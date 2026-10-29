//! Begin and AgingStart prepare on original borrowed resources under one
//! lifecycle reservation. Begin preserves its issued-local gap on rejection.
use super::*;

impl<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    CatalogPreparation<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    pub(super) fn prepare_catalog_aging_start<'i>(
        &self,
        owner: &mut Owner<'c, 's>,
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedCatalog<'i, 'c, 's>> {
        let allocator = self
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let root = target_from_source(self.storage, Plan4Page::Root)?.request;
        let prepared = self.prepare_existing_catalog(
            owner,
            Change::AgingStart,
            None,
            [(allocator, 0), (root, 1)],
            private,
        )?;
        Ok(prepared)
    }

    /// Selects the canonical next free Job from the original current Root.
    /// No reservation or identity is created by this bounded observation.
    pub(in super::super) fn first_catalog_job(&self, owner: &mut Owner<'c, 's>) -> Result<usize> {
        let root = target_from_source(self.storage, Plan4Page::Root)?.request;
        let read = self
            .pool
            .reserve_target_set(owner, &[root])
            .map_err(|e| frame(RetentionPhase::Prepare, e))?
            .try_read()
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let selected = (|| {
            let (_, root) = queue::decode_root_page(
                read.bytes(0)
                    .map_err(|e| frame(RetentionPhase::Prepare, e))?,
                self.storage.profile().spec().namespace,
            )
            .map_err(queue_error)?;
            for offset in 0..64 {
                let ordinal = (root.cursor() as usize + offset) % 64;
                // A Row-scope cell is never free and never a Catalog target.
                if queue::decode_catalog_cell(
                    &self.control.jobs()[ordinal],
                    self.storage.profile().spec().namespace,
                )
                .map_err(queue_error)?
                .is_some_and(|job| job.state() == JobState::Free)
                {
                    return Ok(ordinal);
                }
            }
            Err(unchanged(RetentionPhase::Prepare, RetentionCause::Capacity))
        })();
        merge(
            selected,
            read.release_checked()
                .map_err(|e| frame(RetentionPhase::Prepare, e)),
        )
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RetentionWriter<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// Begin keeps selection, issued-local allocation and application in the
    /// original synchronous gate/source while borrowing its original flusher.
    pub(crate) fn catalog_begin(
        &mut self,
        owner: &mut Owner<'c, 's>,
        status: &Plan4Target,
        private: &mut [u8; PRIVATE_BYTES],
        flusher: &mut RetentionFlusher<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> Result<Observation> {
        self.catalog_lifecycle(owner, CatalogLifecycle::Begin(status), private, flusher)
    }

    /// Start the original Aging obligation inside the same progress call.
    pub(crate) fn catalog_aging_start(
        &mut self,
        owner: &mut Owner<'c, 's>,
        private: &mut [u8; PRIVATE_BYTES],
        flusher: &mut RetentionFlusher<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> Result<Observation> {
        self.catalog_lifecycle(owner, CatalogLifecycle::AgingStart, private, flusher)
    }

    pub(super) fn prepare_catalog_begin<'i>(
        &mut self,
        owner: &mut Owner<'c, 's>,
        status: &Plan4Target,
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedCatalog<'i, 'c, 's>> {
        health(self.storage, self.pool)?;
        if self.control.pending.is_some() || self.control.population.reserved() {
            return Err(fatal(
                self.storage,
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            ));
        }
        let slot = self
            .control
            .free_slot()
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Capacity))?;
        let ordinal = self.catalog_preparation().first_catalog_job(owner)?;
        let allocator = self
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let root = target_from_source(self.storage, Plan4Page::Root)?.request;
        let job = target_from_source(
            self.storage,
            Plan4Page::Jobs {
                index: (ordinal / 15) as u8,
            },
        )?
        .request;
        if status.request.kind != PageKind::TransactionStatus
            || status.request.identity.owner.get() != 2
            || status.request.identity.owner_generation != one()
            || status.request.level != 0
        {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Membership,
            ));
        }
        let read = self
            .pool
            .reserve_target_set(owner, &[status.request])
            .map_err(|e| frame(RetentionPhase::Prepare, e))?
            .try_read()
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let generation = (|| {
            let home = crate::storage::status_home::decode_status_home_page(
                read.bytes(0)
                    .map_err(|e| frame(RetentionPhase::Prepare, e))?,
                status.request.identity,
                self.storage.profile().spec().namespace,
            )
            .map_err(|e| unchanged(RetentionPhase::Prepare, RetentionCause::Page(e)))?;
            let cell = home
                .cell(0)
                .map_err(|e| unchanged(RetentionPhase::Prepare, RetentionCause::Page(e)))?;
            let crate::storage::transaction_record::TransactionStatusCell::Free(generation) =
                cell.core()
            else {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::WrongState,
                ));
            };
            generation
                .next()
                .map_err(|_| unchanged(RetentionPhase::Prepare, RetentionCause::Exhausted))
        })();
        let generation = merge(
            generation,
            read.release_checked()
                .map_err(|e| frame(RetentionPhase::Prepare, e)),
        )?;
        let prefix = self.storage.wal().prefix().map_err(|e| {
            fatal(
                self.storage,
                RetentionPhase::Prepare,
                RetentionCause::Wal(e),
            )
        })?;
        self.control.issued_local = self
            .control
            .issued_local
            .checked_add(1)
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Exhausted))?;
        let actor = TxnRef::new(
            crate::transaction::TxnId {
                database: prefix.stream.database,
                timeline: prefix.stream.timeline,
                writer_epoch: prefix.stream.writer_epoch,
                local: self.control.issued_local,
            },
            generation,
        )
        .map_err(|e| unchanged(RetentionPhase::Prepare, RetentionCause::Page(e)))?;
        let location = StatusSlotRef::new(status.request.identity.page, 0, generation)
            .map_err(|e| unchanged(RetentionPhase::Prepare, RetentionCause::Page(e)))?;
        let prepared = self.catalog_preparation().prepare_existing_catalog(
            owner,
            Change::Begin {
                actor,
                status: location,
            },
            Some(slot),
            [(allocator, 0), (root, 1), (job, 2), (status.request, 4)],
            private,
        )?;
        Ok(prepared)
    }
}

/// Begin's pre-preparation phases. All target reads are consumed before common
/// preparation; health, Root/Job selection and Status selection are sequential.
/// Keep the caller packet live outside each lower closure, including malformed
/// occupied Status paths and duplicate-identity scans (one prior cell at a time).
pub(super) fn selection_source_phases<W: WriterNamespace>() -> (usize, usize, usize) {
    use crate::storage::{
        page::{PageError, PageHeader, PageRef, PageView},
        status_home::{StatusHome, StatusHomePage},
        transaction_record::{TransactionStatus, TransactionStatusCell},
    };
    use std::mem::size_of;
    let caller = 2 * size_of::<CatalogPreparation<'_, '_, '_, '_, '_, '_, '_, '_, W>>()
        + 6 * size_of::<&()>()
        + 3 * size_of::<usize>()
        + 4 * size_of::<TargetRequest>()
        + 3 * size_of::<Result<usize>>()
        + 3 * size_of::<Result<Generation>>()
        + 3 * size_of::<RetentionError>();
    let (waiting, guarded, returned) = dirty_frame::retention_entry_source_phases();
    let health = caller
        + waiting.max(guarded).max(returned)
        + 3 * size_of::<Result<()>>()
        + 2 * size_of::<std::result::Result<bool, dirty_frame::DirtyError>>()
        + 6 * size_of::<&()>();
    let retained_read = size_of::<dirty_frame::ReadTargets<'_, '_, '_, '_, '_, '_, 1>>()
        + 2 * size_of::<&[u8]>()
        + 3 * size_of::<Result<usize>>()
        + 4 * size_of::<&()>();
    let root_decode = queue::root_decode_source_bytes();
    let job_scan = 2 * size_of::<queue::RootView<'_>>()
        + 2 * size_of::<PageHeader>()
        + 2 * size_of::<queue::JobView<'_>>()
        + 3 * size_of::<usize>()
        + size_of::<std::ops::Range<usize>>()
        + queue::source_bytes();
    let root = caller
        + retained_read
        + dirty_frame::target_set_call_bytes::<1>().max(root_decode.max(job_scan));
    // decode_page has no owned page: explicit hash closure, identity/header,
    // slot/scalar walks and returned views. The Status decoder retains its
    // outer occupied value while decoding one previous slot, never all slots.
    let page = 4 * size_of::<foundation::codec::ContentHasher>()
        + 1536
        + 4 * size_of::<PageHeader>()
        + 4 * size_of::<PageIdentity>()
        + 4 * size_of::<PageRef>()
        + 4 * size_of::<PageView<'_>>()
        + 3 * size_of::<std::result::Result<PageView<'_>, PageError>>()
        + size_of::<([u8; 16], [u8; 32], [u8; 24], [u64; 4], [usize; 12])>()
        + 8 * size_of::<&[u8]>()
        + 6 * size_of::<std::result::Result<(), PageError>>();
    let cells = 4 * size_of::<StatusHome>()
        + 4 * size_of::<std::result::Result<StatusHome, PageError>>()
        + 4 * size_of::<TransactionStatusCell>()
        + 4 * size_of::<TransactionStatus>()
        + 3 * size_of::<TxnRef>()
        + 2 * size_of::<crate::transaction::TxnId>()
        + 4 * size_of::<StatusHomePage<'_>>()
        + 3 * size_of::<std::result::Result<StatusHomePage<'_>, PageError>>()
        + 4 * size_of::<std::result::Result<Generation, PageError>>()
        + size_of::<([u8; 8], [usize; 4])>()
        + 2 * size_of::<std::ops::Range<usize>>()
        + 6 * size_of::<&[u8]>()
        + crate::storage::transaction_record::status_codec_source_bytes();
    let status =
        caller + retained_read + dirty_frame::target_set_call_bytes::<1>().max(page + cells);
    (health, root, status)
}
