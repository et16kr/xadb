//! Snapshot lookup copies the Catalog under its original source latch, pins
//! both outgoing references, then resolves Status/Undo without Catalog gates.
use super::super::visibility::{
    self, CapturedCatalogRefs, CatalogLookupLoan, CatalogSource, SnapshotGuard,
};
use super::*;
use crate::storage::{
    catalog_record::{self, CatalogExpectation, CatalogRecordView},
    page::{self, PageRef},
    status_home,
    transaction_record::{
        StatusSlotRef, TransactionState, TransactionStatusCell, TxnRef, VersionStamp,
    },
    undo_record::{
        self, UndoBeforeKind, UndoCreator, UndoEdge, UndoPayload, UndoRecordExpectation,
        UndoTargetKind, UndoTargetView,
    },
    undo_ref::UndoRef,
};
use foundation::identity::Generation;

#[allow(
    clippy::large_enum_variant,
    reason = "the borrowed record view stays inline and is charged by source_bytes; lookup must not allocate"
)]
pub(crate) enum CatalogLookupStep<'a> {
    Found(CatalogRecordView<'a>),
    Missing,
    FlushRequired { target: Plan4Target },
}
fn visibility_error(error: visibility::VisibilityError) -> RetentionError {
    RetentionError {
        phase: RetentionPhase::Census,
        effect: error.effect,
        cause: RetentionCause::Visibility(error),
    }
}
fn release_refs(refs: CapturedCatalogRefs<'_, '_, '_, '_>) -> Result<()> {
    let tx = refs.transaction.map_or(Ok(()), |pin| {
        pin.release_checked().map_err(visibility_error)
    });
    let undo = refs.undo.map_or(Ok(()), |pin| {
        pin.release_checked().map_err(visibility_error)
    });
    merge(tx, undo)
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

    /// The result borrows only the caller's private Catalog copy. All transient
    /// references and physical guards are explicitly released before return.
    /// A dirty victim is returned to the original flusher; retry uses a fresh
    /// source capture with the same still-registered snapshot.
    pub(in crate::storage) fn lookup_catalog<'q>(
        &'b self,
        owner: &mut Owner<'c, 's>,
        snapshot: &SnapshotGuard<'_, '_, 'c, 's>,
        object: ObjectId,
        cursor: &mut Plan4ResidencyCursor,
        private: &'q mut [u8; 425_984],
    ) -> Result<CatalogLookupStep<'q>> {
        health(self.storage, self.pool)?;
        self.pool
            .check_retention_reader(owner)
            .map_err(|e| frame(RetentionPhase::Census, e))?;
        let visibility = self
            .visibility
            .ok_or_else(|| unchanged(RetentionPhase::Census, RetentionCause::WrongState))?;
        if !snapshot.confirms(visibility).map_err(visibility_error)? {
            return Err(unchanged(
                RetentionPhase::Census,
                RetentionCause::WrongState,
            ));
        }
        let loan = CatalogLookupLoan::split(private);
        let mut lease = visibility
            .reserve_references(owner)
            .map_err(visibility_error)?;
        let capture = (|| {
            let guard = self.read_catalog(owner)?;
            let copied = (|| {
                let source = CatalogSource::from_guard(&guard, object)?;
                let value = guard.value(object)?;
                let len = value.map_or(0, |value| value.len());
                if len > 4_588 {
                    return Err(fatal(
                        self.storage,
                        RetentionPhase::Census,
                        RetentionCause::Invariant,
                    ));
                }
                let refs = lease.capture_catalog(source).map_err(visibility_error)?;
                loan.catalog.fill(0);
                if let Some(value) = value {
                    loan.catalog[..len].copy_from_slice(value);
                }
                Ok((len, refs))
            })();
            merge(copied, guard.release_checked())
        })();
        let result = match capture {
            Ok((len, refs)) => {
                #[cfg(test)]
                crate::transaction::concurrency::catalog_test_support::hit(
                    self.pool.bound_core(),
                    crate::transaction::concurrency::catalog_test_support::Point::LookupCaptured,
                );
                let selected = (|| {
                    if len == 0 {
                        return Ok(CatalogLookupStep::Missing);
                    }
                    let value = catalog_record::decode_catalog_record(
                        &loan.catalog[..len],
                        CatalogExpectation {
                            namespace: self.storage.profile().spec().namespace,
                            object,
                        },
                    )
                    .map_err(|e| {
                        fatal(
                            self.storage,
                            RetentionPhase::Census,
                            RetentionCause::Catalog(e),
                        )
                    })?;
                    let header = value.header();
                    let visible = match header.stamp {
                        VersionStamp::Committed(q) => q.get() <= snapshot.q(),
                        VersionStamp::Transaction(reference) => {
                            if refs.transaction.as_ref().map(|pin| pin.reference())
                                != Some(reference)
                            {
                                return Err(fatal(
                                    self.storage,
                                    RetentionPhase::Census,
                                    RetentionCause::Invariant,
                                ));
                            }
                            let (request, location) =
                                self.catalog_status_location(owner, reference)?;
                            if let Some(target) =
                                self.lookup_resident(owner, request, cursor, loan.loader)?
                            {
                                return Ok(CatalogLookupStep::FlushRequired { target });
                            }
                            let guard = self.read(owner, &Plan4Target { request })?;
                            let state = (|| {
                                let page = status_home::decode_status_home_page(
                                    guard.bytes()?,
                                    request.identity,
                                    self.storage.profile().spec().namespace,
                                )
                                .map_err(|e| {
                                    fatal(
                                        self.storage,
                                        RetentionPhase::Census,
                                        RetentionCause::Page(e),
                                    )
                                })?;
                                let home = page.lookup(reference, location).map_err(|e| {
                                    fatal(
                                        self.storage,
                                        RetentionPhase::Census,
                                        RetentionCause::Page(e),
                                    )
                                })?;
                                let TransactionStatusCell::Occupied(status) = home.core() else {
                                    return Err(fatal(
                                        self.storage,
                                        RetentionPhase::Census,
                                        RetentionCause::Invariant,
                                    ));
                                };
                                Ok(status.state())
                            })();
                            match merge(state, guard.release_checked())? {
                                TransactionState::Committed(q) => q.get() <= snapshot.q(),
                                TransactionState::Active
                                | TransactionState::Aborting(_)
                                | TransactionState::Aborted => false,
                            }
                        }
                    };
                    if visible {
                        return Ok(if header.retired {
                            CatalogLookupStep::Missing
                        } else {
                            CatalogLookupStep::Found(value)
                        });
                    }
                    // The CREATE subset has one canonical Missing before-image.
                    // A newer committed frozen cell with no Undo is absent at Q.
                    let Some(reference) = header.undo else {
                        if matches!(header.stamp, VersionStamp::Committed(_)) {
                            return Ok(CatalogLookupStep::Missing);
                        }
                        return Err(fatal(
                            self.storage,
                            RetentionPhase::Census,
                            RetentionCause::Invariant,
                        ));
                    };
                    if refs.undo.as_ref().map(|pin| pin.reference()) != Some(reference) {
                        return Err(fatal(
                            self.storage,
                            RetentionPhase::Census,
                            RetentionCause::Invariant,
                        ));
                    }
                    let request = self.catalog_undo_location(owner, reference)?;
                    if let Some(target) =
                        self.lookup_resident(owner, request, cursor, loan.loader)?
                    {
                        return Ok(CatalogLookupStep::FlushRequired { target });
                    }
                    let guard = self.read(owner, &Plan4Target { request })?;
                    let missing = (|| {
                        let page =
                            page::decode_page(guard.bytes()?, request.identity, PageKind::Undo)
                                .map_err(|e| {
                                    fatal(
                                        self.storage,
                                        RetentionPhase::Census,
                                        RetentionCause::Page(e),
                                    )
                                })?;
                        let cell = page.cell(usize::from(reference.slot())).ok_or_else(|| {
                            fatal(
                                self.storage,
                                RetentionPhase::Census,
                                RetentionCause::Invariant,
                            )
                        })?;
                        let expected = UndoRecordExpectation {
                            namespace: self.storage.profile().spec().namespace,
                            reference,
                            target: UndoTargetView {
                                kind: UndoTargetKind::Catalog,
                                owner: ObjectId::new(5).expect("Catalog"),
                                generation: Generation::new(1).expect("one"),
                                entity: object.get(),
                                arity: 0,
                                key: &[],
                            },
                        };
                        let record = undo_record::decode_undo_record(cell, expected, &[])
                            .and_then(|view| view.live_before())
                            .map_err(|e| {
                                fatal(
                                    self.storage,
                                    RetentionPhase::Census,
                                    RetentionCause::Undo(e),
                                )
                            })?;
                        let VersionStamp::Transaction(actor) = header.stamp else {
                            return Err(fatal(
                                self.storage,
                                RetentionPhase::Census,
                                RetentionCause::Invariant,
                            ));
                        };
                        if record.creator
                            != (UndoCreator {
                                id: actor.id(),
                                status_generation: actor.generation(),
                            })
                            || record.before_kind != UndoBeforeKind::Missing
                            || record.before_stamp.is_some()
                            || record.before_logical.is_some()
                            || !matches!(record.payload, UndoPayload::None)
                            || record.edges != [UndoEdge::Never; 2]
                            || record.incoming != 2
                            || record.sequence == 0
                            || record.mutation_sequence != 1
                            || record.release.cursor != 0
                            || record.release.chunks_released != 0
                            || record.release.next.is_some()
                        {
                            return Err(fatal(
                                self.storage,
                                RetentionPhase::Census,
                                RetentionCause::Invariant,
                            ));
                        }
                        Ok(CatalogLookupStep::Missing)
                    })();
                    merge(missing, guard.release_checked())
                })();
                merge(selected, release_refs(refs))
            }
            Err(error) => Err(error),
        };
        merge(result, lease.release_checked().map_err(visibility_error))
    }

    fn lookup_resident(
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
    fn catalog_status_location(
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
    fn catalog_undo_location(
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
        catalog_record::CatalogHeader,
        page::{PageHeader, PageView},
        transaction_record::TransactionStatus,
        undo_record::{UndoRecord, UndoRecordView},
    };
    use std::mem::size_of;
    let retained = size_of::<CatalogLookupLoan<'_>>()
        + size_of::<visibility::ReferenceLease<'_, '_, '_, '_>>()
        + 2 * size_of::<CapturedCatalogRefs<'_, '_, '_, '_>>()
        + size_of::<Result<(usize, CapturedCatalogRefs<'_, '_, '_, '_>)>>()
        + 3 * size_of::<Result<CatalogLookupStep<'_>>>()
        + 2 * size_of::<CatalogRecordView<'_>>()
        + size_of::<CatalogHeader>()
        + size_of::<(
            &RetentionReader<'_, '_, '_, '_, '_, '_, '_, '_, W>,
            &mut Owner<'_, '_>,
            &SnapshotGuard<'_, '_, '_, '_>,
            ObjectId,
            &mut Plan4ResidencyCursor,
            &mut [u8; 425984],
        )>()
        + 4 * size_of::<Result<()>>();
    let capture = super::catalog_source_bytes::<W>()
        + visibility::source_census().reference
        + size_of::<CatalogSource<'_, '_, '_>>()
        + 2 * size_of::<Result<Option<&[u8]>>>()
        + size_of::<usize>();
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
    retained
        + capture
            .max(location)
            .max(super::super::lookup_load::source_bytes::<W>())
            .max(read)
            .max(visibility::source_census().peak_leaf)
}
