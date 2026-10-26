//! Snapshot lookup copies the Catalog under its original source latch, pins
//! both outgoing references, then resolves Status/Undo without Catalog gates.
use super::super::visibility::{self, CatalogSource, SnapshotGuard};
use super::*;
use crate::storage::{
    page::PageRef,
    status_home,
    transaction_record::{StatusSlotRef, TransactionState, TransactionStatusCell, TxnRef},
    undo_record::{self, UndoRecordExpectation, UndoTargetView},
    undo_ref::UndoRef,
};

use crate::storage::catalog::read::CatalogLookupStep;
fn visibility_error(error: visibility::VisibilityError) -> RetentionError {
    RetentionError {
        phase: RetentionPhase::Census,
        effect: error.effect,
        cause: RetentionCause::Visibility(error),
    }
}
impl<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    RetentionReader<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    pub(in crate::storage) fn catalog_snapshot(
        &self,
        owner: &mut Owner<'c, 's>,
    ) -> Result<SnapshotGuard<'_, 'a, 'c, 's>> {
        health(self.storage, self.pool)?;
        self.pool
            .check_retention_reader(owner)
            .map_err(|e| frame(RetentionPhase::Census, e))?;
        let visibility = self
            .visibility
            .ok_or_else(|| unchanged(RetentionPhase::Census, RetentionCause::WrongState))?;
        let reserved = visibility.reserve_snapshot().map_err(visibility_error)?;
        visibility
            .register_current(owner, reserved)
            .map_err(visibility_error)
    }

    /// Synchronous compatibility entry. Only this private call constructs the
    /// adapter and immediately consumes it in the Catalog product module. The
    /// caller's actual snapshot stays borrowed until all refs are released.
    pub(in crate::storage) fn lookup_catalog<'q>(
        &self,
        owner: &mut Owner<'c, 's>,
        snapshot: &SnapshotGuard<'_, '_, 'c, 's>,
        object: ObjectId,
        cursor: &mut Plan4ResidencyCursor,
        private: &'q mut [u8; 425_984],
    ) -> Result<CatalogLookupStep<'q>> {
        let operation =
            super::operation::CatalogReadOperation::begin(self, owner, snapshot, private)?;
        crate::storage::catalog::read::lookup(operation, object, cursor)
    }

    pub(super) fn lookup_resident(
        &self,
        owner: &mut Owner<'c, 's>,
        request: TargetRequest,
        cursor: &mut Plan4ResidencyCursor,
        scratch: &mut [u8],
    ) -> Result<Option<Plan4Target>> {
        match self.load_lookup_target(owner, request, cursor, &[], scratch)? {
            RetentionResidencyStep::Resident { .. } => Ok(None),
            RetentionResidencyStep::FlushRequired { target, .. } => Ok(Some(target)),
        }
    }

    /// Resolve a location from the installed source while the actual TxnPin is
    /// held by lookup. This is a location observation, never an install permit.
    pub(super) fn catalog_status_location(
        &self,
        owner: &mut Owner<'c, 's>,
        reference: TxnRef,
    ) -> Result<(TargetRequest, StatusSlotRef)> {
        self.catalog_reference_location(owner, |request, bytes| {
            if request.kind != PageKind::TransactionStatus { return Ok(None); }
            let page = status_home::decode_status_home_page(bytes, request.identity, self.storage.profile().spec().namespace)
                .map_err(|e| fatal(self.storage, RetentionPhase::Census, RetentionCause::Page(e)))?;
            let mut found = None;
            for slot in 0..page.len() {
                if matches!(page.cell(slot).map_err(|e| fatal(self.storage, RetentionPhase::Census, RetentionCause::Page(e)))?.core(), TransactionStatusCell::Occupied(status) if status.reference() == reference) {
                    let location = StatusSlotRef::new(request.identity.page, slot as u16, reference.generation()).map_err(|e| fatal(self.storage, RetentionPhase::Census, RetentionCause::Page(e)))?;
                    if found.replace((request, location)).is_some() { return Err(fatal(self.storage, RetentionPhase::Census, RetentionCause::Invariant)); }
                }
            }
            Ok(found)
        })
    }
    pub(super) fn catalog_undo_location(
        &self,
        owner: &mut Owner<'c, 's>,
        reference: UndoRef,
    ) -> Result<TargetRequest> {
        self.catalog_reference_location(owner, |request, _| {
            Ok(
                (request.kind == PageKind::Undo && request.identity.page == reference.page())
                    .then_some(request),
            )
        })
    }
    fn catalog_reference_location<T>(
        &self,
        owner: &mut Owner<'c, 's>,
        mut select: impl FnMut(TargetRequest, &[u8; PAGE_BYTES]) -> Result<Option<T>>,
    ) -> Result<T> {
        let cut = self.history.source_cut()?;
        let durable = cut.ensure_durable(self.storage)?;
        let mut audit = self.history.replay_scope(owner)?;
        let result = match audit.checkout() {
            Ok(mut token) => {
                let selected = (|| {
                    history::replay_lookup_cut(
                        self.storage,
                        &cut,
                        durable.as_ref(),
                        token.bytes_mut(),
                    )?;
                    let allocator = self
                        .pool
                        .target_request(0)
                        .map_err(|e| frame(RetentionPhase::Census, e))?;
                    let allocation = allocation_page::decode_allocation_page(
                        history::arena_page(token.bytes(), 0),
                        allocator.identity,
                        self.storage.profile().spec().namespace,
                    )
                    .map_err(|e| {
                        fatal(
                            self.storage,
                            RetentionPhase::Census,
                            RetentionCause::Allocation(e),
                        )
                    })?;
                    let mut found = None;
                    for index in 1..64 {
                        let AllocationEntry::Allocated {
                            generation,
                            object,
                            object_generation,
                            kind,
                        } = allocation.entry(index).map_err(|e| {
                            fatal(
                                self.storage,
                                RetentionPhase::Census,
                                RetentionCause::Allocation(e),
                            )
                        })?
                        else {
                            continue;
                        };
                        let request = TargetRequest {
                            identity: PageIdentity {
                                database: allocator.identity.database,
                                page: PageRef {
                                    file: allocator.identity.page.file,
                                    page_no: index as u64,
                                    incarnation: generation,
                                },
                                owner: object,
                                owner_generation: object_generation,
                            },
                            kind,
                            level: 0,
                        };
                        if let Some(value) =
                            select(request, history::arena_page(token.bytes(), index))?
                            && found.replace(value).is_some()
                        {
                            return Err(fatal(
                                self.storage,
                                RetentionPhase::Census,
                                RetentionCause::Invariant,
                            ));
                        }
                    }
                    cut.unchanged()?;
                    found.ok_or_else(|| {
                        fatal(
                            self.storage,
                            RetentionPhase::Census,
                            RetentionCause::Invariant,
                        )
                    })
                })();
                merge(selected, token.return_checked())
            }
            Err(error) => Err(error),
        };
        merge(result, audit.finish_checked())
    }
}

/// Complete lookup call closure, excluding its original retained owner/pool
/// and the separately budgeted 425984-byte caller loan. Actual source guards
/// end before Status/Undo resolution; no physical Catalog latch survives I/O.
pub(in super::super) fn source_bytes<W: WriterNamespace>() -> usize {
    use crate::storage::{
        page::{PageHeader, PageView},
        transaction_record::TransactionStatus,
        undo_record::{UndoRecord, UndoRecordView},
    };
    use std::mem::size_of;
    let retained =
        super::operation::held_source_bytes::<W>() + crate::storage::catalog::read::source_bytes();
    let capture = super::catalog_source_bytes::<W>()
        + visibility::source_census().reference
        + size_of::<CatalogSource<'_, '_, '_>>()
        + 2 * size_of::<Result<Option<&[u8]>>>()
        + size_of::<usize>()
        + 2 * size_of::<Result<super::operation::CatalogCandidate>>()
        + 2 * size_of::<super::operation::CatalogCandidate>();
    // The installed cut, original checkout and result survive both phases.
    // replay_lookup_cut returns before allocator/status selection declares its
    // page views and found/request values. Runtime metadata calls release their
    // mutex guards before the WAL replay; they do not construct RuntimeHistory.
    let location_held = size_of::<history::runtime::LookupSourceCut<'_, '_, '_, '_>>()
        + size_of::<Option<DurableWal<'_>>>()
        + size_of::<history::runtime::ReplayScope<'_, '_, '_, '_>>()
        + size_of::<history::runtime::ReplayLoan<'_, '_, '_, '_>>()
        + 3 * size_of::<Result<(TargetRequest, StatusSlotRef)>>()
        + 16 * size_of::<&()>()
        + size_of::<(TxnRef, UndoRef)>();
    let location_select = 2 * size_of::<Option<(TargetRequest, StatusSlotRef)>>()
        + 2 * size_of::<TargetRequest>()
        + size_of::<allocation_page::AllocationPage<'_>>()
        + size_of::<status_home::StatusHomePage<'_>>()
        + 2 * size_of::<status_home::StatusHome>()
        + size_of::<TransactionStatus>()
        + size_of::<(usize, PageHeader, PageView<'_>)>()
        // The existing physical status/undo read bound conservatively covers
        // allocation envelope, occupied Status decode and callback returns.
        + super::undo_read_source_bytes()
        + history::runtime::lookup_source_bytes();
    let location = location_held
        + history::source_bytes::<W>()
            .peak
            .max(
                history::runtime::lookup_source_bytes()
                    + crate::storage::wal_io::DURABILITY_CALL_BYTES,
            )
            .max(location_select);
    let read = super::undo_read_source_bytes()
        + 3 * size_of::<UndoRecord<'_>>()
        + 2 * size_of::<UndoRecordView<'_>>()
        + 2 * size_of::<std::result::Result<UndoRecordView<'_>, undo_record::UndoError>>()
        + 2 * size_of::<UndoRecordExpectation<'_>>()
        + 2 * size_of::<UndoTargetView<'_>>()
        + size_of::<status_home::StatusHomePage<'_>>()
        + 2 * size_of::<status_home::StatusHome>()
        + 2 * size_of::<Result<TransactionState>>()
        + crate::storage::semantic_catalog::schema_source_bytes();
    super::operation::entry_source_bytes::<W>().max(
        retained
            + capture
                .max(location)
                .max(super::super::lookup_load::source_bytes::<W>())
                .max(read)
                .max(super::operation::decode_source_bytes())
                .max(visibility::source_census().peak_leaf),
    )
}
