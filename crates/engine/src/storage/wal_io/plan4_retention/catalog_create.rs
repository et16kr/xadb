//! CREATE operations on short disjoint borrows of the original resources.
//! The reservation retains the exact disjoint resources and original flush lease.
use super::*;

/// A continuous exclusive borrow of the original flush owner. The selected
/// generation and pair are fixed before the claim, never refreshed at Cancel.
pub(super) struct CatalogPairIoLease<'r, 'c, 's, 'w> {
    pub(super) original: &'r mut flush::RetentionFlushControl<'c, 's, 'w>,
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    fence: &'s SessionFence,
    selected_generation: u64,
    pair: [TargetRequest; 2],
}
impl<'r, 'c, 's, 'w> CatalogPairIoLease<'r, 'c, 's, 'w> {
    fn acquire<W: WriterNamespace>(
        storage: &'s MutableStorageOwner<'_, 'w, W>,
        core: &'c PageConcurrency<'s>,
        original: &'r mut flush::RetentionFlushControl<'c, 's, 'w>,
        pair: [TargetRequest; 2],
    ) -> Result<Self> {
        original.check_catalog_pair_io::<W>(&storage.binding, storage.session_fence())?;
        let selected_generation = original.io.snapshot().known.generation;
        Ok(Self {
            original,
            core,
            binding: &storage.binding,
            fence: storage.session_fence(),
            selected_generation,
            pair,
        })
    }
    pub(super) fn check<W: WriterNamespace>(
        &mut self,
        storage: &MutableStorageOwner<'_, 'w, W>,
        core: &PageConcurrency<'s>,
        pair: &[TargetRequest; 2],
    ) -> Result<()> {
        if !std::ptr::eq(self.core, core)
            || !std::ptr::eq(self.binding, &storage.binding)
            || !std::ptr::eq(self.fence, storage.session_fence())
            || self.pair != *pair
            || self.selected_generation != self.original.io.snapshot().known.generation
        {
            return Err(fatal(
                storage,
                RetentionPhase::Prepare,
                RetentionCause::BindingMismatch,
            ));
        }
        self.original
            .check_catalog_pair_io::<W>(self.binding, self.fence)
    }
}

impl<'r, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    CatalogCreateAccess<'r, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    pub(super) fn reserve(
        mut self,
        owner: &'r mut Owner<'c, 's>,
        actor: TxnRef,
        undo: &Plan4Target,
        definition: &'r crate::api::TableSpec,
        private: &'r mut [u8; PRIVATE_BYTES],
    ) -> Result<ReservedCatalogCreate<'r, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>> {
        // Preserve deep pending/population and planner error precedence.
        let plan = self.plan_catalog_create(owner, actor, undo, definition, private)?;
        let Self {
            storage,
            pool,
            control,
            history,
            visibility,
            flush,
        } = self;
        let mut io = CatalogPairIoLease::acquire(storage, pool.bound_core(), flush, plan.pair)?;
        let (key, reserve) = CatalogCreateAccess {
            storage,
            pool,
            control: &mut *control,
            history,
            visibility,
            flush: &mut *io.original,
        }
        .reserve_planned(owner, &plan, private)?;
        io.check::<W>(storage, pool.bound_core(), &plan.pair)
            .map_err(|error| fatal(storage, error.phase, error.cause))?;
        Ok(ReservedCatalogCreate {
            storage,
            pool,
            control,
            history,
            visibility,
            io,
            owner,
            private,
            definition,
            plan,
            key,
            reserve,
            active: true,
        })
    }

    fn reserve_planned(
        &mut self,
        owner: &mut Owner<'c, 's>,
        plan: &CreatePlan,
        private: &mut [u8; PRIVATE_BYTES],
    ) -> Result<(TxPairClaimKey<'c, 's>, PageLsn)> {
        let slot = self.control.slot(plan.plan.actor).ok_or_else(|| {
            unchanged(RetentionPhase::Prepare, RetentionCause::MissingTransaction)
        })?;
        let prepared = self.catalog_preparation().prepare_existing_catalog(
            owner,
            Change::Reserve(&plan.plan),
            Some(slot),
            plan.selected(),
            private,
        )?;
        if prepared.payload.observation.job_ordinal != Some(plan.ordinal) {
            return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Busy));
        }
        self.flush.io.preflight::<W>().map_err(|e| RetentionError {
            phase: RetentionPhase::Prepare,
            effect: e.effect,
            cause: RetentionCause::Flush(e),
        })?;
        let candidates = self
            .pool
            .catalog_pair_candidates(&plan.pair, 1, &plan.existing)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        if candidates.requires_flush() {
            return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Busy));
        }
        let reserve = PageLsn(prepared.payload.observation.header.lsn);
        let authority = TxPairAuthority {
            core: self.pool.bound_core(),
            binding: &self.storage.binding,
            fence: self.storage.session_fence(),
            actor: plan.plan.actor,
            reserve,
            existing: &plan.existing,
            pair: &plan.pair,
        };
        let mut key = self
            .pool
            .try_prepare_tx_pair(&authority, &candidates)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        if let Err(error) = self.apply_pair_catalog(&key, owner, prepared) {
            if error.effect == Effect::Unchanged && self.control.pending.is_none() {
                self.return_catalog_pair(
                    owner,
                    &mut key,
                    plan,
                    reserve,
                    super::super::TxPairCancelKind::Unstaged,
                    None,
                )
                .map_err(|cleanup| fatal(self.storage, cleanup.phase, cleanup.cause))?;
                return Err(error);
            }
            return Err(fatal(self.storage, error.phase, error.cause));
        }
        Ok((key, reserve))
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    CatalogCreateAccess<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    pub(super) fn cancel(
        &mut self,
        key: &mut TxPairClaimKey<'c, 's>,
        plan: &CreatePlan,
        reserve: PageLsn,
        owner: &mut Owner<'c, 's>,
        private: &mut [u8; PRIVATE_BYTES],
    ) -> Result<()> {
        let storage = self.storage;
        self.flush.io.preflight::<W>().map_err(|e| RetentionError {
            phase: RetentionPhase::Prepare,
            effect: e.effect,
            cause: RetentionCause::Flush(e),
        })?;
        let slot = self.control.slot(plan.plan.actor).ok_or_else(|| {
            unchanged(RetentionPhase::Prepare, RetentionCause::MissingTransaction)
        })?;
        let prepared = self.prepare_pair_catalog(
            &*key,
            owner,
            Change::Cancel {
                ordinal: plan.ordinal,
            },
            Some(slot),
            plan.selected(),
            private,
        )?;
        let epoch_change = self
            .visibility
            .as_ref()
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::WrongState))?
            .prepare_epoch_change(&prepared, storage.profile())
            .map_err(|e| RetentionError {
                phase: RetentionPhase::Prepare,
                effect: e.effect,
                cause: RetentionCause::Visibility(e),
            })?;
        let observation = self.apply_pair_catalog(&*key, owner, prepared)?;
        let source = self.history.source_cut()?;
        let requirement = source
            .requirement()
            .ok_or_else(|| fatal(storage, RetentionPhase::Install, RetentionCause::Invariant))?;
        if requirement.location().lsn != observation.header.lsn
            || self
                .control
                .durable
                .as_ref()
                .is_none_or(|durable| !durable.confirms(&requirement))
        {
            return Err(fatal(
                storage,
                RetentionPhase::Durable,
                RetentionCause::Invariant,
            ));
        }
        source.unchanged()?;
        self.return_catalog_pair(
            owner,
            key,
            plan,
            reserve,
            super::super::TxPairCancelKind::Cancelled,
            Some(requirement),
        )
        .map_err(|error| fatal(storage, error.phase, error.cause))?;
        self.visibility
            .as_ref()
            .expect("checked original visibility")
            .apply_epoch_change(epoch_change)
            .map_err(|e| {
                fatal(
                    storage,
                    RetentionPhase::Install,
                    RetentionCause::Visibility(e),
                )
            })
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "original reservation resources remain explicit borrowed inputs"
    )]
    pub(super) fn initialize(
        &mut self,
        key: &mut TxPairClaimKey<'c, 's>,
        plan: &CreatePlan,
        reserve: PageLsn,
        owner: &mut Owner<'c, 's>,
        private: &mut [u8; PRIVATE_BYTES],
        definition: &crate::api::TableSpec,
    ) -> Result<ObjectHandle> {
        use crate::storage::catalog_record::CatalogTableInput;
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        let slot = self.control.slot(plan.plan.actor).ok_or_else(|| {
            unchanged(RetentionPhase::Prepare, RetentionCause::MissingTransaction)
        })?;
        let mut prepared = self.prepare_pair_catalog(
            &*key,
            owner,
            Change::Forward {
                ordinal: plan.ordinal,
                definition: CatalogTableInput::Spec(definition),
            },
            Some(slot),
            plan.selected(),
            private,
        )?;
        prepared.add_initialized_pair(plan.pair)?;
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|e| fatal(storage, RetentionPhase::Prepare, RetentionCause::Wal(e)))?;
        prepared.check(owner, self.pool, self.control, prefix)?;
        let requests: &[TargetRequest; 9] =
            prepared.targets().try_into().expect("whole Forward set");
        let authority = TxPairAuthority {
            core: self.pool.bound_core(),
            binding: &storage.binding,
            fence: storage.session_fence(),
            actor: plan.plan.actor,
            reserve,
            existing: &plan.existing,
            pair: &plan.pair,
        };
        if !key.matches_authority(&authority) {
            return Err(fatal(
                storage,
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            ));
        }
        let view = prepared.layout(storage.profile())?;
        let job_page = queue::decode_jobs_page(
            &prepared.payload.after[2],
            storage.profile().spec().namespace,
            (plan.ordinal / 15) as u8,
        )
        .map_err(queue_error)?;
        let job_after: &[u8; 1024] = job_page
            .cell(plan.ordinal % 15)
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?
            .try_into()
            .map_err(|_| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
        let after: [Option<&[u8; PAGE_BYTES]>; 9] = std::array::from_fn(|at| prepared.after(at));
        let transaction_before = content_digest(&self.control.row(slot)[..1536]);
        let aging_before = content_digest(&self.control.row(control::AGING_SLOT)[..1536]);
        let job_before = content_digest(&self.control.jobs()[plan.ordinal]);
        let inventory_before = content_digest(&self.control.bytes[JOB_END..INVENTORY_END]);
        let inventory_after = prepared
            .payload
            .inventory_after()
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
        let mut write = self
            .pool
            .claim_tx_pair_targets(owner, &*key, requests)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?
            .try_write()
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let installed = (|| {
            for (at, after_image) in after.iter().enumerate() {
                let actual = write
                    .observe(at)
                    .map_err(|e| frame(RetentionPhase::Prepare, e))?
                    .map(|o| o.frame);
                if actual != prepared.stamp(at) {
                    return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Busy));
                }
                if after_image.is_some()
                    && actual.is_some_and(|stamp| {
                        stamp.modification_counter == u64::MAX
                            || stamp.page_lsn.0 >= prepared.payload.observation.header.lsn
                    })
                {
                    return Err(unchanged(
                        RetentionPhase::Prepare,
                        RetentionCause::Exhausted,
                    ));
                }
            }
            write
                .enter_stage()
                .map_err(|e| frame(RetentionPhase::Stage, e))?;
            let requirement = match storage.wal().stage_verified_required(
                prepared.payload.prefix,
                prepared.payload.observation.header,
                prepared.payload.frame,
            ) {
                Ok(requirement) => requirement,
                Err(error) if error.effect == Effect::Unchanged => {
                    write
                        .accept_stage_rejection(super::super::TxPairStageRejection { key: &*key })
                        .map_err(|e| frame(RetentionPhase::Stage, e))?;
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
            catalog_mark_staged(&prepared, self.control, requirement)?;
            self.control.row_mut(slot)[control::PHASE] = control::STAGED;
            let permit = TxPairInstallPermit {
                key: &*key,
                requirement,
                targets: requests,
                before: &prepared.stamps,
                after: &after,
            };
            let control = TxPairControlCommit {
                authority: &authority,
                original: &mut *self.control,
                requirement,
                transaction_slot: slot,
                job_ordinal: plan.ordinal,
                transaction_before,
                aging_before,
                job_before,
                inventory_before,
                transaction_after: view.descriptor(0).expect("Forward Tx descriptor"),
                aging_after: view.descriptor(1).expect("Forward Aging descriptor"),
                job_after,
                inventory_after,
                applied: false,
            };
            let completed = write
                .install_forward(&permit, control)
                .map_err(|e| frame(RetentionPhase::Install, e))?;
            self.history.installed(requirement)?;
            Ok((requirement, completed))
        })();
        let (requirement, completed) = merge(
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
        self.pool
            .finish_tx_pair(key, completed)
            .map_err(|e| fatal(storage, RetentionPhase::Install, RetentionCause::Frame(e)))?;
        self.control.durable = Some(durable);
        self.control.pending = None;
        Ok(plan.object)
    }

    pub(super) fn return_catalog_pair(
        &mut self,
        owner: &mut Owner<'c, 's>,
        key: &mut TxPairClaimKey<'c, 's>,
        plan: &CreatePlan,
        reserve: PageLsn,
        kind: super::super::TxPairCancelKind,
        requirement: Option<WalRequirement<'s>>,
    ) -> Result<()> {
        let storage = self.storage;
        self.flush.io.preflight::<W>().map_err(|e| RetentionError {
            phase: RetentionPhase::Prepare,
            effect: e.effect,
            cause: RetentionCause::Flush(e),
        })?;
        if self.flush.phase != flush::RetentionFlushPhase::Ready {
            return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Busy));
        }
        let allocator = self
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let read = self
            .pool
            .reserve_tx_pair_existing(owner, key, &[allocator])
            .map_err(|e| frame(RetentionPhase::Prepare, e))?
            .try_read()
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let stamp = read
            .observe(0)
            .map(|observation| observation.frame)
            .map_err(|e| frame(RetentionPhase::Prepare, e));
        let allocator_after = merge(
            stamp,
            read.release_checked()
                .map_err(|e| frame(RetentionPhase::Prepare, e)),
        )?;
        let authority = TxPairAuthority {
            core: self.pool.bound_core(),
            binding: &storage.binding,
            fence: storage.session_fence(),
            actor: plan.plan.actor,
            reserve,
            existing: &plan.existing,
            pair: &plan.pair,
        };
        let selected_generation = self.flush.io.snapshot().known.generation;
        let io = super::super::PairIoQuiescence {
            original: &mut *self.flush,
            binding: &storage.binding,
            fence: storage.session_fence(),
            selected_generation,
            pair: plan.pair,
        };
        self.pool
            .cancel_tx_pair(
                key,
                super::super::TxPairCancelPermit {
                    authority: &authority,
                    io,
                    kind,
                    requirement,
                    allocator_after,
                },
            )
            .map_err(|e| frame(RetentionPhase::Install, e))
    }

    pub(super) fn plan_catalog_create(
        &mut self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
        undo: &Plan4Target,
        definition: &crate::api::TableSpec,
        private: &mut [u8; PRIVATE_BYTES],
    ) -> Result<CreatePlan> {
        use crate::storage::{
            catalog_record, page,
            transaction_record::{LogicalRowVersion, VersionStamp},
            tree_page::{self, TreeExpectation, edit::LeafEdit},
            undo_record,
            undo_ref::UndoRef,
        };
        self.health()?;
        if self.control.pending.is_some()
            || self.control.population.reserved()
            || self.flush.phase != flush::RetentionFlushPhase::Ready
        {
            return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Busy));
        }
        if undo.request.kind != PageKind::Undo
            || undo.request.level != 0
            || undo.request.identity.owner.get() != 3
            || undo.request.identity.owner_generation != one()
        {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Membership,
            ));
        }
        let terminal = self.actor_catalog_job(actor, true)?;
        let terminal = queue::decode_job(
            &self.control.jobs()[terminal],
            self.storage.profile().spec().namespace,
        )
        .map_err(queue_error)?;
        let status = terminal
            .status()
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
        let ordinal = self.first_catalog_job(owner)?;
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
        let anchor = target_from_source(self.storage, Plan4Page::CatalogAnchor)?.request;
        let catalog = target_from_source(self.storage, Plan4Page::CatalogLeaf)?.request;
        let mut existing = [
            allocator,
            root,
            jobs,
            anchor,
            catalog,
            self.catalog_status_request(status),
            undo.request,
        ];
        existing.sort_unstable_by_key(|request| target_rank(*request));
        let position = |request| {
            existing
                .iter()
                .position(|r| *r == request)
                .expect("existing role")
        };
        let read = self
            .pool
            .reserve_target_set(owner, &existing)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?
            .try_read()
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let planned = (|| {
            let namespace = self.storage.profile().spec().namespace;
            let allocation = allocation_page::decode_allocation_page(
                read.bytes(position(allocator))
                    .map_err(|e| frame(RetentionPhase::Prepare, e))?,
                allocator.identity,
                namespace,
            )
            .map_err(|e| unchanged(RetentionPhase::Prepare, RetentionCause::Allocation(e)))?;
            let (_, root) = queue::decode_root_page(
                read.bytes(position(root))
                    .map_err(|e| frame(RetentionPhase::Prepare, e))?,
                namespace,
            )
            .map_err(queue_error)?;
            let object = ObjectHandle {
                object: ObjectId::new(root.object_high().checked_add(1).ok_or_else(|| {
                    unchanged(RetentionPhase::Prepare, RetentionCause::Exhausted)
                })?)
                .map_err(|_| unchanged(RetentionPhase::Prepare, RetentionCause::Exhausted))?,
                generation: one(),
                schema_epoch: SchemaEpoch::new(1).expect("first schema"),
            };
            let mut pair = [allocator.identity.page; 2];
            let mut count = 0;
            for index in 9..64 {
                if let AllocationEntry::Free(generation) = allocation.entry(index).map_err(|e| {
                    unchanged(RetentionPhase::Prepare, RetentionCause::Allocation(e))
                })? {
                    pair[count] = PageRef {
                        page_no: index as u64,
                        incarnation: generation.next().map_err(|_| {
                            unchanged(RetentionPhase::Prepare, RetentionCause::Exhausted)
                        })?,
                        ..allocator.identity.page
                    };
                    count += 1;
                    if count == 2 {
                        break;
                    }
                }
            }
            if count != 2 {
                return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Capacity));
            }
            let undo_page = page::decode_page(
                read.bytes(position(undo.request))
                    .map_err(|e| frame(RetentionPhase::Prepare, e))?,
                undo.request.identity,
                PageKind::Undo,
            )
            .map_err(|e| unchanged(RetentionPhase::Prepare, RetentionCause::Page(e)))?;
            if undo_page.len() != 1 {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Membership,
                ));
            }
            let free = undo_page
                .cell(0)
                .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
            if free.len() != 432 {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Invariant,
                ));
            }
            let old_generation = Generation::new(u64::from_le_bytes(
                free[8..16].try_into().expect("checked Free cell"),
            ))
            .map_err(|_| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
            undo_record::decode_free_undo_record(free, old_generation)
                .map_err(|e| unchanged(RetentionPhase::Prepare, RetentionCause::Undo(e)))?;
            let reference = UndoRef::new(
                undo.request.identity.page,
                0,
                old_generation
                    .next()
                    .map_err(|_| unchanged(RetentionPhase::Prepare, RetentionCause::Exhausted))?,
            )
            .map_err(|e| unchanged(RetentionPhase::Prepare, RetentionCause::Page(e)))?;
            // The first schema/leaf-capacity validation happens before Reserve.
            // Both temporary cells borrow the existing private frame lane.
            let (value, rest) = private.split_at_mut(catalog_record::MAX_CATALOG_CELL_BYTES);
            let cell = &mut rest[..semantic_catalog::MAX_CELL];
            let length = catalog_record::encode_catalog_record(
                catalog_record::CatalogExpectation {
                    namespace,
                    object: object.object,
                },
                catalog_record::CatalogHeader {
                    identity: object,
                    retired: false,
                    stamp: VersionStamp::Transaction(actor),
                    logical: LogicalRowVersion::new(actor.id(), 1)
                        .map_err(|e| unchanged(RetentionPhase::Prepare, RetentionCause::Page(e)))?,
                    undo: Some(reference),
                    anchor: pair[0],
                },
                catalog_record::CatalogDefinition::Table(definition),
                value,
            )
            .map_err(|e| unchanged(RetentionPhase::Prepare, RetentionCause::Catalog(e)))?;
            let projection = semantic_catalog::catalog_projection_digest(
                namespace,
                object.object,
                Some(&value[..length]),
            )
            .map_err(semantic)?;
            let leaf = tree_page::decode_tree_page(
                read.bytes(position(catalog))
                    .map_err(|e| frame(RetentionPhase::Prepare, e))?,
                TreeExpectation {
                    identity: catalog.identity,
                    kind: PageKind::CatalogLeaf,
                    level: 0,
                    arity: 0,
                },
            )
            .map_err(|e| {
                unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::CatalogSemantic(semantic_catalog::Error::Tree(e)),
                )
            })?;
            let encoded = tree_page::encode_leaf_cell(
                PageKind::CatalogLeaf,
                0,
                &object.object.get().to_be_bytes(),
                &value[..length],
                cell,
            )
            .map_err(|e| {
                unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::CatalogSemantic(semantic_catalog::Error::Tree(e)),
                )
            })?;
            semantic_catalog::images::validate_catalog_leaf(&leaf, namespace).map_err(semantic)?;
            let mut cells = [&[][..]; semantic_catalog::images::CATALOG_EDIT_CELLS];
            if leaf
                .prepare_leaf_edit(LeafEdit::Insert(encoded), &mut cells)
                .map_err(|e| {
                    unchanged(
                        RetentionPhase::Prepare,
                        RetentionCause::CatalogSemantic(semantic_catalog::Error::LeafEdit(e)),
                    )
                })?
                .needs_split()
            {
                return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Capacity));
            }
            Ok(CreatePlan {
                plan: semantic_catalog::queue::ReservePlan {
                    actor,
                    status,
                    undo: reference,
                    pair,
                    catalog_bytes: (length + 12) as u32,
                    projection,
                },
                ordinal,
                object,
                existing,
                pair: std::array::from_fn(|at| {
                    request(
                        PageIdentity {
                            database: allocator.identity.database,
                            page: pair[at],
                            owner: object.object,
                            owner_generation: object.generation,
                        },
                        if at == 0 {
                            PageKind::TreeAnchor
                        } else {
                            PageKind::RowLeaf
                        },
                    )
                }),
            })
        })();
        merge(
            planned,
            read.release_checked()
                .map_err(|e| frame(RetentionPhase::Prepare, e)),
        )
    }

    pub(super) fn apply_pair_catalog(
        &mut self,
        key: &TxPairClaimKey<'c, 's>,
        owner: &mut Owner<'c, 's>,
        prepared: PreparedCatalog<'_, 'c, 's>,
    ) -> Result<Observation> {
        let mut application = self.catalog_application();
        application.access = CatalogTargetAccess::Pair(key);
        application.apply_existing_catalog(owner, prepared)
    }

    pub(super) fn prepare_pair_catalog<'i, const N: usize>(
        &self,
        key: &TxPairClaimKey<'c, 's>,
        owner: &mut Owner<'c, 's>,
        change: Change<'_>,
        slot: Option<usize>,
        selected: [(TargetRequest, u8); N],
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedCatalog<'i, 'c, 's>> {
        let mut preparation = self.catalog_preparation();
        preparation.access = CatalogTargetAccess::Pair(key);
        preparation.prepare_existing_catalog(owner, change, slot, selected, private)
    }

    pub(super) fn first_catalog_job(&self, owner: &mut Owner<'c, 's>) -> Result<usize> {
        self.catalog_preparation().first_catalog_job(owner)
    }

    pub(super) fn actor_catalog_job(&self, actor: TxnRef, terminal: bool) -> Result<usize> {
        self.catalog_preparation()
            .actor_catalog_job(actor, terminal)
    }

    pub(super) fn catalog_status_request(&self, status: StatusSlotRef) -> TargetRequest {
        self.catalog_preparation().catalog_status_request(status)
    }

    fn health(&self) -> Result<()> {
        health(self.storage, self.pool)?;
        if self.control.pending.is_some() || self.control.population.reserved() {
            return Err(fatal(
                self.storage,
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            ));
        }
        Ok(())
    }
    fn catalog_preparation(&self) -> CatalogPreparation<'_, 'a, 's, 'c, 'p, 'd, 'm, 'w, W> {
        CatalogPreparation {
            storage: self.storage,
            pool: self.pool,
            control: self.control,
            history: self.history,
            visibility: self.visibility,
            access: CatalogTargetAccess::Ordinary,
        }
    }
}

#[cfg(test)]
impl CatalogPairIoLease<'_, '_, '_, '_> {
    pub(super) fn test_identity(&self) -> (usize, u64) {
        (
            std::ptr::from_ref(&*self.original) as usize,
            self.selected_generation,
        )
    }
    /// Deliberate corruptions/temporary refusal, never a successful authority.
    pub(super) fn test_fault(&mut self, kind: u8) {
        match kind {
            0 => self.original.phase = flush::RetentionFlushPhase::WaitingSource,
            1 => self.original.phase = flush::RetentionFlushPhase::Ready,
            2 => self.selected_generation += 1,
            3 => self.pair.swap(0, 1),
            4 => {
                static FOREIGN: AtomicU8 = AtomicU8::new(0);
                self.binding = &FOREIGN;
            }
            _ => panic!("unknown lease fault"),
        }
    }
}
