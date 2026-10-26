//! Exact latest-home load. The original pool publishes only after both selected
//! physical reads match a canonical image and the allocator snapshot survives.
use super::*;
use crate::storage::{
    allocation_page::{self, AllocationEntry},
    dirty_frame::{StatusCandidateKind, StatusLoadSnapshot},
    page::PageRef,
};

pub(in crate::storage) struct RetentionLoadPermit<'a, 'c, 's> {
    snapshot: &'a StatusLoadSnapshot<'c, 's>,
    index: usize,
    allocator: &'a [u8; PAGE_BYTES],
    bytes: &'a [u8; PAGE_BYTES],
    request: TargetRequest,
}
impl<'a, 'c, 's> RetentionLoadPermit<'a, 'c, 's> {
    pub(in crate::storage) fn snapshot(&self) -> &'a StatusLoadSnapshot<'c, 's> {
        self.snapshot
    }
    pub(in crate::storage) fn index(&self) -> usize {
        self.index
    }
    pub(in crate::storage) fn allocator(&self) -> &'a [u8; PAGE_BYTES] {
        self.allocator
    }
    pub(in crate::storage) fn bytes(&self) -> &'a [u8; PAGE_BYTES] {
        self.bytes
    }
    pub(in crate::storage) fn request(&self) -> TargetRequest {
        self.request
    }
}
impl<'s, 'c, W: WriterNamespace> Plan4Retention<'_, 's, 'c, '_, '_, '_, '_, '_, W> {
    pub(crate) fn status_target(&mut self, page: PageRef) -> Result<Plan4Target> {
        self.system_target(page, super::population::RetentionPageKind::Status)
    }
    pub(crate) fn undo_target(&mut self, page: PageRef) -> Result<Plan4Target> {
        self.system_target(page, super::population::RetentionPageKind::FreeUndo)
    }
    fn system_target(
        &mut self,
        page: PageRef,
        expected: super::population::RetentionPageKind,
    ) -> Result<Plan4Target> {
        self.health()?;
        let storage = self.inner.storage;
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|e| fatal(storage, RetentionPhase::Load, RetentionCause::Wal(e)))?;
        history::validate_live(
            storage,
            &self.control.ledger,
            self.history.exclusive()?,
            prefix,
        )?;
        let allocator = self
            .inner
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Load, e))?
            .identity;
        let allocation = allocation_page::decode_allocation_page(
            history::arena_page(self.history.exclusive()?, 0),
            allocator,
            storage.profile().spec().namespace,
        )
        .map_err(|e| fatal(storage, RetentionPhase::Load, RetentionCause::Allocation(e)))?;
        if !(9..64).contains(&page.page_no) || page.file != allocator.page.file {
            return Err(unchanged(RetentionPhase::Load, RetentionCause::Membership));
        }
        let AllocationEntry::Allocated {
            generation,
            object,
            object_generation,
            kind,
        } = allocation
            .entry(page.page_no as usize)
            .map_err(|e| fatal(storage, RetentionPhase::Load, RetentionCause::Allocation(e)))?
        else {
            return Err(unchanged(RetentionPhase::Load, RetentionCause::Membership));
        };
        if generation != page.incarnation
            || object != expected.object()
            || object_generation.get() != 1
            || kind != expected.kind()
        {
            return Err(unchanged(RetentionPhase::Load, RetentionCause::Membership));
        }
        Ok(Plan4Target {
            request: TargetRequest {
                identity: PageIdentity {
                    database: allocator.database,
                    page,
                    owner: object,
                    owner_generation: object_generation,
                },
                kind,
                level: 0,
            },
        })
    }
    pub(crate) fn load_into(
        &mut self,
        index: usize,
        expected: &Plan4Target,
    ) -> Result<super::super::Plan4LoadObservation> {
        let storage = self.inner.storage;
        let _unwind = storage.session_fence().unwind_guard();
        self.health()?;
        if self.flush.phase != RetentionFlushPhase::Ready {
            return Err(unchanged(RetentionPhase::Load, RetentionCause::Busy));
        }
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|e| fatal(storage, RetentionPhase::Load, RetentionCause::Wal(e)))?;
        history::validate_live(
            storage,
            &self.control.ledger,
            self.history.exclusive()?,
            prefix,
        )?;
        let request = expected.request;
        let page_no = usize::try_from(request.identity.page.page_no)
            .map_err(|_| unchanged(RetentionPhase::Load, RetentionCause::Membership))?;
        if !(1..64).contains(&page_no) {
            return Err(unchanged(RetentionPhase::Load, RetentionCause::Membership));
        }
        let replay = self.history.exclusive()?;
        let canonical = history::arena_page(replay, page_no);
        crate::storage::page::decode_page(canonical, request.identity, request.kind)
            .map_err(|_| unchanged(RetentionPhase::Load, RetentionCause::Membership))?;
        let pages: &mut [[u8; PAGE_BYTES]; 7] = self
            .inner
            .operation
            .as_chunks_mut::<PAGE_BYTES>()
            .0
            .try_into()
            .expect("original operation");
        let [allocator, first, second, root, chunk, _, _] = pages;
        let snapshot = self
            .inner
            .pool
            .prepare_retention_load_at(index, allocator)
            .map_err(|e| frame(RetentionPhase::Load, e))?;
        if allocator != history::arena_page(replay, 0) {
            return Err(fatal(
                storage,
                RetentionPhase::Load,
                RetentionCause::Invariant,
            ));
        }
        if self
            .inner
            .pool
            .resident_address_present(request)
            .map_err(|e| frame(RetentionPhase::Load, e))?
        {
            return Err(unchanged(RetentionPhase::Load, RetentionCause::Membership));
        }
        let allocator_identity = snapshot.allocator().identity;
        for output in [first, &mut *second] {
            let mut reader = storage
                .core
                .open_reader(root, chunk)
                .map_err(|e| fatal(storage, RetentionPhase::Load, RetentionCause::PageIo(e)))?;
            reader
                .read_retention_home(
                    request.identity,
                    request.kind,
                    request.level,
                    allocator_identity,
                    output,
                )
                .map_err(|e| fatal(storage, RetentionPhase::Load, RetentionCause::PageIo(e)))?;
            if output != canonical {
                return Err(fatal(
                    storage,
                    RetentionPhase::Load,
                    RetentionCause::Invariant,
                ));
            }
        }
        if storage
            .wal()
            .prefix()
            .map_err(|e| fatal(storage, RetentionPhase::Load, RetentionCause::Wal(e)))?
            != prefix
        {
            return Err(fatal(
                storage,
                RetentionPhase::Load,
                RetentionCause::Invariant,
            ));
        }
        let permit = RetentionLoadPermit {
            snapshot: &snapshot,
            index,
            allocator,
            bytes: second,
            request,
        };
        let after = self
            .inner
            .pool
            .install_retention_load(permit)
            .map_err(|e| frame(RetentionPhase::Load, e))?;
        Ok(super::super::Plan4LoadObservation {
            before: snapshot.payload(),
            after,
        })
    }
    pub(crate) fn residency_step(
        &mut self,
        expected: &Plan4Target,
        cursor: &mut Plan4ResidencyCursor,
    ) -> Result<RetentionResidencyStep> {
        self.prepare_required(expected, cursor, &[expected.request])
    }
    pub(super) fn prepare_required(
        &mut self,
        expected: &Plan4Target,
        cursor: &mut Plan4ResidencyCursor,
        required: &[TargetRequest],
    ) -> Result<RetentionResidencyStep> {
        self.health()?;
        let candidate = self
            .inner
            .pool
            .retention_candidate(expected.request, cursor.next, required)
            .map_err(|e| frame(RetentionPhase::Load, e))?;
        match candidate.kind {
            StatusCandidateKind::Resident => Ok(RetentionResidencyStep::Resident {
                index: candidate.index,
                frame: candidate.frame.expect("resident"),
                loaded: false,
            }),
            StatusCandidateKind::Flush => {
                let request = self
                    .inner
                    .pool
                    .target_request(candidate.index)
                    .map_err(|e| frame(RetentionPhase::Load, e))?;
                Ok(RetentionResidencyStep::FlushRequired {
                    target: Plan4Target { request },
                    rec_lsn: candidate.frame.and_then(|s| s.rec_lsn).ok_or_else(|| {
                        fatal(
                            self.inner.storage,
                            RetentionPhase::Load,
                            RetentionCause::Invariant,
                        )
                    })?,
                })
            }
            StatusCandidateKind::Load => {
                let loaded = self.load_into(candidate.index, expected)?;
                cursor.next = candidate.next;
                Ok(RetentionResidencyStep::Resident {
                    index: candidate.index,
                    frame: loaded.after,
                    loaded: true,
                })
            }
        }
    }
    /// Caller completes any returned real flush and calls again. Required
    /// addresses are protected throughout each finite scan; no pin is held
    /// while a missing page is loaded. The actual writer pins the whole set.
    pub(crate) fn prepare_empty_targets(
        &mut self,
        owner: &Owner<'c, 's>,
        status: &Plan4Target,
        job: JobSlotRef,
        cursor: &mut Plan4ResidencyCursor,
    ) -> Result<Option<RetentionResidencyStep>> {
        self.health()?;
        self.inner
            .pool
            .check_retention_reader(owner)
            .map_err(|e| frame(RetentionPhase::Load, e))?;
        let root = self.target(Plan4Page::Root)?;
        let jobs = self.target(Plan4Page::Jobs {
            index: (job.page().page_no - 2) as u8,
        })?;
        let required = [
            self.inner
                .pool
                .target_request(0)
                .map_err(|e| frame(RetentionPhase::Load, e))?,
            root.request,
            jobs.request,
            status.request,
        ];
        for target in [&root, &jobs, status] {
            let step = self.prepare_required(target, cursor, &required)?;
            if matches!(step, RetentionResidencyStep::FlushRequired { .. }) {
                return Ok(Some(step));
            }
        }
        Ok(None)
    }
    pub(crate) fn prepare_catalog(
        &mut self,
        owner: &Owner<'c, 's>,
        cursor: &mut Plan4ResidencyCursor,
    ) -> Result<Option<RetentionResidencyStep>> {
        self.health()?;
        self.inner
            .pool
            .check_retention_reader(owner)
            .map_err(|e| frame(RetentionPhase::Load, e))?;
        let anchor = self.target(Plan4Page::CatalogAnchor)?;
        let leaf = self.target(Plan4Page::CatalogLeaf)?;
        let required = [
            self.inner
                .pool
                .target_request(0)
                .map_err(|e| frame(RetentionPhase::Load, e))?,
            anchor.request,
            leaf.request,
        ];
        for target in [&anchor, &leaf] {
            let step = self.prepare_required(target, cursor, &required)?;
            if matches!(step, RetentionResidencyStep::FlushRequired { .. }) {
                return Ok(Some(step));
            }
        }
        Ok(None)
    }

    /// Prepare CREATE's existing set together. Separate empty-transaction and
    /// Catalog scans may otherwise replace pages prepared by the previous scan.
    pub(crate) fn prepare_catalog_create_targets(
        &mut self,
        owner: &Owner<'c, 's>,
        status: &Plan4Target,
        undo: &Plan4Target,
        job: JobSlotRef,
        cursor: &mut Plan4ResidencyCursor,
    ) -> Result<Option<RetentionResidencyStep>> {
        self.health()?;
        self.inner
            .pool
            .check_retention_reader(owner)
            .map_err(|e| frame(RetentionPhase::Load, e))?;
        let root = self.target(Plan4Page::Root)?;
        let jobs = self.target(Plan4Page::Jobs {
            index: (job.page().page_no - 2) as u8,
        })?;
        let anchor = self.target(Plan4Page::CatalogAnchor)?;
        let leaf = self.target(Plan4Page::CatalogLeaf)?;
        let required = [
            self.inner
                .pool
                .target_request(0)
                .map_err(|e| frame(RetentionPhase::Load, e))?,
            root.request,
            jobs.request,
            anchor.request,
            leaf.request,
            status.request,
            undo.request,
        ];
        for target in [&root, &jobs, &anchor, &leaf, status, undo] {
            let step = self.prepare_required(target, cursor, &required)?;
            if matches!(step, RetentionResidencyStep::FlushRequired { .. }) {
                return Ok(Some(step));
            }
        }
        // Reserve later selects two unprotected victims with the same rule and
        // refuses a dirty one. Surface it here so the caller's original flusher
        // can clean it, e.g. an earlier CREATE's pair in the same epoch.
        for candidate in self
            .inner
            .pool
            .retention_pair_room(1, &required)
            .map_err(|e| frame(RetentionPhase::Load, e))?
        {
            if candidate.kind == StatusCandidateKind::Flush {
                let request = self
                    .inner
                    .pool
                    .target_request(candidate.index)
                    .map_err(|e| frame(RetentionPhase::Load, e))?;
                return Ok(Some(RetentionResidencyStep::FlushRequired {
                    target: Plan4Target { request },
                    rec_lsn: candidate.frame.and_then(|s| s.rec_lsn).ok_or_else(|| {
                        fatal(
                            self.inner.storage,
                            RetentionPhase::Load,
                            RetentionCause::Invariant,
                        )
                    })?,
                }));
            }
        }
        Ok(None)
    }
}
