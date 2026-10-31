//! One lookup borrows its call scope and owns only the actual reference lease,
//! captured references and caller scratch. The scope outlives result consumption.
use super::super::source::OperationSourceAdmission;
use super::super::transaction::RowStatementView;
use super::super::visibility::{
    self, CapturedCatalogRefs, CatalogLookupLoan, CatalogSource, ReferenceLease, SnapshotGuard,
};
use super::*;
use crate::storage::{
    catalog_record::{self, CatalogExpectation, CatalogHeader, CatalogRecordView},
    page, status_home,
    transaction_record::{
        TransactionState, TransactionStatus, TransactionStatusCell, VersionStamp,
    },
    undo_record::{
        self, UndoBeforeKind, UndoCreator, UndoEdge, UndoPayload, UndoRecordExpectation,
        UndoTargetKind, UndoTargetView,
    },
};
use foundation::identity::Generation;

#[derive(Debug)]
pub(crate) enum ReadStep<T, Target = Plan4Target> {
    Ready(T),
    FlushRequired { target: Target },
}
/// A source observation, not a pin, grant or caller-constructible capability.
pub(crate) struct CatalogCandidate {
    object: ObjectId,
    len: usize,
    header: Option<CatalogHeader>,
}
impl CatalogCandidate {
    pub(in crate::storage) fn header(&self) -> Option<CatalogHeader> {
        self.header
    }
}
pub(crate) struct MissingCatalogEvidence {
    _private: (),
}

pub(crate) struct CatalogReadOperation<
    'u,
    'r,
    'v,
    'j,
    'i,
    'b,
    'a,
    's,
    'c,
    'p,
    'd,
    'm,
    'w,
    W: WriterNamespace,
> {
    backend: &'r ReadPort<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>,
    source: &'r OperationSourceAdmission<'b, 'a, 'c, 's>,
    owner: &'u mut Owner<'c, 's>,
    snapshot: &'r SnapshotGuard<'v, 'j, 'c, 's>,
    loader: &'i mut [u8; 409_600],
    copy: Option<&'i mut [u8; 16_384]>,
    lease: Option<ReferenceLease<'b, 'a, 'c, 's>>,
    captured: Option<CapturedCatalogRefs<'b, 'a, 'c, 's>>,
    selected: Option<(ObjectId, usize)>,
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
impl<'u, 'r, 'v, 'j, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    CatalogReadOperation<'u, 'r, 'v, 'j, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    pub(super) fn begin(
        backend: &'r ReadPort<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>,
        owner: &'u mut Owner<'c, 's>,
        snapshot: &'r SnapshotGuard<'v, 'j, 'c, 's>,
        source: &'r OperationSourceAdmission<'b, 'a, 'c, 's>,
        private: &'i mut [u8; 425_984],
    ) -> Result<Self> {
        let visibility = backend
            .visibility
            .ok_or_else(|| unchanged(RetentionPhase::Census, RetentionCause::WrongState))?;
        let lease = visibility
            .reserve_references(owner)
            .map_err(visibility_error)?;
        let loan = CatalogLookupLoan::split(private);
        Ok(Self {
            backend,
            source,
            owner,
            snapshot,
            loader: loan.loader,
            copy: Some(loan.catalog),
            lease: Some(lease),
            captured: None,
            selected: None,
        })
    }
    fn invariant(&self) -> RetentionError {
        fatal(
            self.backend.storage,
            RetentionPhase::Census,
            RetentionCause::Invariant,
        )
    }
    fn record(&self, object: ObjectId, len: usize) -> Result<CatalogRecordView<'_>> {
        let bytes = self.copy.as_deref().ok_or_else(|| self.invariant())?;
        catalog_record::decode_catalog_record(
            &bytes[..len],
            CatalogExpectation {
                namespace: self.backend.storage.profile().spec().namespace,
                object,
            },
        )
        .map_err(|e| {
            fatal(
                self.backend.storage,
                RetentionPhase::Census,
                RetentionCause::Catalog(e),
            )
        })
    }
    fn check(&self, candidate: &CatalogCandidate) -> Result<()> {
        if self.selected != Some((candidate.object, candidate.len)) || self.captured.is_none() {
            return Err(self.invariant());
        }
        let header = if candidate.len == 0 {
            None
        } else {
            Some(self.record(candidate.object, candidate.len)?.header())
        };
        if header != candidate.header {
            return Err(self.invariant());
        }
        Ok(())
    }
    pub(in crate::storage) fn q(&self) -> Result<u64> {
        health(self.backend.storage, self.backend.pool)?;
        let visibility = self.backend.visibility.ok_or_else(|| self.invariant())?;
        if !self
            .snapshot
            .confirms(visibility)
            .map_err(visibility_error)?
        {
            return Err(self.invariant());
        }
        Ok(self.snapshot.q())
    }
    /// The actual statement must name this operation's original snapshot and
    /// registries. No actor scalar grants own-write visibility.
    pub(in crate::storage) fn check_statement(
        &self,
        statement: Option<&RowStatementView<'_, '_, 'a, 'c, 's>>,
    ) -> Result<()> {
        if let Some(statement) = statement {
            let snapshot =
                statement.catalog_snapshot(self.backend.isolation, self.backend.visibility)?;
            // Compare addresses only; the two borrowed guard types have
            // independent invariant arena lifetimes in the legacy API.
            if !std::ptr::eq::<()>(
                std::ptr::from_ref(snapshot).cast(),
                std::ptr::from_ref(self.snapshot).cast(),
            ) {
                return Err(self.invariant());
            }
            statement.check_catalog_read()?;
        }
        Ok(())
    }
    pub(in crate::storage) fn capture_catalog(
        &mut self,
        object: ObjectId,
    ) -> Result<CatalogCandidate> {
        if self.selected.is_some() {
            return Err(self.invariant());
        }
        let guard = self
            .backend
            .read_catalog(&mut *self.owner)
            .map_err(|error| {
                if self.source.resident() {
                    super::resident::selected_error(self.backend.storage, error)
                } else {
                    error
                }
            })?;
        let copied = (|| {
            let source = CatalogSource::from_guard(&guard, object)?;
            let value = guard.value(object)?;
            let len = value.map_or(0, |value| value.len());
            if len > 4_588 {
                return Err(fatal(
                    self.backend.storage,
                    RetentionPhase::Census,
                    RetentionCause::Invariant,
                ));
            }
            let lease = self.lease.as_mut().ok_or_else(|| {
                fatal(
                    self.backend.storage,
                    RetentionPhase::Census,
                    RetentionCause::Invariant,
                )
            })?;
            let refs = lease.capture_catalog(source).map_err(visibility_error)?;
            self.captured = Some(refs);
            let copy = self.copy.as_deref_mut().ok_or_else(|| {
                fatal(
                    self.backend.storage,
                    RetentionPhase::Census,
                    RetentionCause::Invariant,
                )
            })?;
            copy.fill(0);
            if let Some(value) = value {
                copy[..len].copy_from_slice(value);
            }
            Ok(len)
        })();
        let len = merge(copied, guard.release_checked())?;
        self.selected = Some((object, len));
        #[cfg(test)]
        crate::transaction::concurrency::catalog_test_support::hit(
            self.backend.pool.bound_core(),
            crate::transaction::concurrency::catalog_test_support::Point::LookupCaptured,
        );
        let header = if len == 0 {
            None
        } else {
            Some(self.record(object, len)?.header())
        };
        Ok(CatalogCandidate {
            object,
            len,
            header,
        })
    }
    pub(in crate::storage) fn status(
        &mut self,
        candidate: &CatalogCandidate,
        cursor: &mut Plan4ResidencyCursor,
        statement: Option<&RowStatementView<'_, '_, 'a, 'c, 's>>,
    ) -> Result<ReadStep<(TransactionState, bool)>> {
        self.check_statement(statement)?;
        self.check(candidate)?;
        let Some(CatalogHeader {
            stamp: VersionStamp::Transaction(reference),
            ..
        }) = candidate.header
        else {
            return Err(self.invariant());
        };
        if self
            .captured
            .as_ref()
            .and_then(|r| r.transaction.as_ref())
            .map(|pin| pin.reference())
            != Some(reference)
        {
            return Err(self.invariant());
        }
        let (request, location) = if self.source.resident() {
            self.backend.resident_status_location(
                &mut *self.owner,
                reference,
                (&mut self.loader[..PAGE_BYTES])
                    .try_into()
                    .expect("caller allocator page"),
            )?
        } else {
            self.backend
                .catalog_status_location(&mut *self.owner, reference)?
        };
        #[cfg(test)]
        if self.source.resident() {
            crate::transaction::concurrency::catalog_test_support::hit(
                self.backend.pool.bound_core(),
                crate::transaction::concurrency::catalog_test_support::Point::ResidentStatusLocated,
            );
        }
        if !self.source.resident()
            && let Some(target) =
                self.backend
                    .lookup_resident(&mut *self.owner, request, cursor, self.loader)?
        {
            return Ok(ReadStep::FlushRequired { target });
        }
        let guard = self
            .backend
            .read(&mut *self.owner, &Plan4Target { request })
            .map_err(|error| {
                if self.source.resident() {
                    super::resident::selected_error(self.backend.storage, error)
                } else {
                    error
                }
            })?;
        let state = (|| {
            let page = status_home::decode_status_home_page(
                guard.bytes()?,
                request.identity,
                self.backend.storage.profile().spec().namespace,
            )
            .map_err(|e| {
                fatal(
                    self.backend.storage,
                    RetentionPhase::Census,
                    RetentionCause::Page(e),
                )
            })?;
            let home = page.lookup(reference, location).map_err(|e| {
                fatal(
                    self.backend.storage,
                    RetentionPhase::Census,
                    RetentionCause::Page(e),
                )
            })?;
            let TransactionStatusCell::Occupied(status) = home.core() else {
                return Err(fatal(
                    self.backend.storage,
                    RetentionPhase::Census,
                    RetentionCause::Invariant,
                ));
            };
            Ok(status)
        })();
        let status = merge(state, guard.release_checked())?;
        // Release the physical guard before checking the original isolation
        // slot. Keep the complete Status, including head, through this check.
        let own = match statement {
            Some(statement) => statement
                .check_own_catalog(candidate.header.ok_or_else(|| self.invariant())?, status)?,
            None => false,
        };
        Ok(ReadStep::Ready((status.state(), own)))
    }
    pub(in crate::storage) fn missing_before(
        &mut self,
        candidate: &CatalogCandidate,
        cursor: &mut Plan4ResidencyCursor,
    ) -> Result<ReadStep<MissingCatalogEvidence>> {
        self.check(candidate)?;
        let Some(header) = candidate.header else {
            return Err(self.invariant());
        };
        let Some(reference) = header.undo else {
            return Err(self.invariant());
        };
        if self
            .captured
            .as_ref()
            .and_then(|r| r.undo.as_ref())
            .map(|pin| pin.reference())
            != Some(reference)
        {
            return Err(self.invariant());
        }
        let request = if self.source.resident() {
            self.backend.resident_undo_location(
                &mut *self.owner,
                reference,
                (&mut self.loader[..PAGE_BYTES])
                    .try_into()
                    .expect("caller allocator page"),
            )?
        } else {
            self.backend
                .catalog_undo_location(&mut *self.owner, reference)?
        };
        #[cfg(test)]
        if self.source.resident() {
            crate::transaction::concurrency::catalog_test_support::hit(
                self.backend.pool.bound_core(),
                crate::transaction::concurrency::catalog_test_support::Point::ResidentUndoLocated,
            );
        }
        if !self.source.resident()
            && let Some(target) =
                self.backend
                    .lookup_resident(&mut *self.owner, request, cursor, self.loader)?
        {
            return Ok(ReadStep::FlushRequired { target });
        }
        let guard = self
            .backend
            .read_catalog_undo(&mut *self.owner, &Plan4Target { request })
            .map_err(|error| {
                if self.source.resident() {
                    super::resident::selected_error(self.backend.storage, error)
                } else {
                    error
                }
            })?;
        let missing = (|| {
            let page = page::decode_page(guard.bytes()?, request.identity, PageKind::Undo)
                .map_err(|e| {
                    fatal(
                        self.backend.storage,
                        RetentionPhase::Census,
                        RetentionCause::Page(e),
                    )
                })?;
            let cell = page.cell(usize::from(reference.slot())).ok_or_else(|| {
                fatal(
                    self.backend.storage,
                    RetentionPhase::Census,
                    RetentionCause::Invariant,
                )
            })?;
            let expected = UndoRecordExpectation {
                namespace: self.backend.storage.profile().spec().namespace,
                reference,
                target: UndoTargetView {
                    kind: UndoTargetKind::Catalog,
                    owner: ObjectId::new(5).expect("Catalog"),
                    generation: Generation::new(1).expect("one"),
                    entity: candidate.object.get(),
                    arity: 0,
                    key: &[],
                },
            };
            let record = undo_record::decode_undo_record(cell, expected, &[])
                .and_then(|view| view.live_before())
                .map_err(|e| {
                    fatal(
                        self.backend.storage,
                        RetentionPhase::Census,
                        RetentionCause::Undo(e),
                    )
                })?;
            let VersionStamp::Transaction(actor) = header.stamp else {
                return Err(fatal(
                    self.backend.storage,
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
                    self.backend.storage,
                    RetentionPhase::Census,
                    RetentionCause::Invariant,
                ));
            }
            Ok(ReadStep::Ready(MissingCatalogEvidence { _private: () }))
        })();
        merge(missing, guard.release_checked())
    }
    fn close_refs(&mut self) -> Result<()> {
        let refs = self.captured.take().map_or(Ok(()), release_refs);
        let lease = self.lease.take().map_or(Ok(()), |lease| {
            lease.release_checked().map_err(visibility_error)
        });
        merge(refs, lease)
    }
    pub(in crate::storage) fn finish_found(
        mut self,
        candidate: CatalogCandidate,
    ) -> Result<CatalogRecordView<'i>> {
        let checked = self.check(&candidate).and_then(|()| {
            if candidate.len == 0 {
                Err(self.invariant())
            } else {
                Ok(())
            }
        });
        let cleanup = self.close_refs();
        merge(checked, cleanup)?;
        let namespace = self.backend.storage.profile().spec().namespace;
        let bytes = self.copy.take().ok_or_else(|| self.invariant())?;
        catalog_record::decode_catalog_record(
            &bytes[..candidate.len],
            CatalogExpectation {
                namespace,
                object: candidate.object,
            },
        )
        .map_err(|e| {
            fatal(
                self.backend.storage,
                RetentionPhase::Census,
                RetentionCause::Catalog(e),
            )
        })
    }
    pub(in crate::storage) fn finish_missing(mut self) -> Result<()> {
        self.close_refs()
    }
    pub(in crate::storage) fn finish_error(mut self, error: RetentionError) -> RetentionError {
        match merge::<()>(Err(error), self.close_refs()) {
            Err(error) => error,
            Ok(()) => unreachable!("original error retained"),
        }
    }
}
impl<W: WriterNamespace> Drop
    for CatalogReadOperation<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>
{
    fn drop(&mut self) {
        let _ = self.close_refs();
    }
}

/// The call scope remains live through lookup and selected result consumption.
pub(super) fn held_source_bytes<W: WriterNamespace>() -> usize {
    use std::mem::size_of;
    super::scope::held_source_bytes::<W>()
        + size_of::<CatalogReadOperation<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>>()
        + 4 * size_of::<Result<()>>()
        + 2 * size_of::<RetentionError>()
        // missing_before's copied header survives location/LOAD through actor validation.
        + size_of::<CatalogHeader>()
        + size_of::<TransactionStatus>()
        + size_of::<Option<&RowStatementView<'_, '_, '_, '_, '_>>>()
        + 16 * size_of::<&()>()
}
pub(super) fn entry_source_bytes<W: WriterNamespace>() -> usize {
    use std::mem::size_of;
    2 * size_of::<CatalogReadOperation<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>>()
        + size_of::<
            Result<CatalogReadOperation<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>>,
        >()
        // begin returns through scope.operation; retain both boundary Results.
        + 2 * size_of::<Result<CatalogReadOperation<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>>>()
        + size_of::<CatalogLookupLoan<'_>>()
        + 2 * size_of::<ReferenceLease<'_, '_, '_, '_>>()
        + size_of::<Result<ReferenceLease<'_, '_, '_, '_>>>()
        + 4 * size_of::<Result<()>>()
        + 16 * size_of::<&()>()
        + visibility::source_census().peak_leaf
        + super::scope::entry_source_bytes::<W>()
}

/// record()/check()/finish_found decoding ends before location replay or LOAD.
/// Its views are nested scratch, not retained across those mutually exclusive phases.
pub(super) fn decode_source_bytes() -> usize {
    use std::mem::size_of;
    2 * size_of::<CatalogRecordView<'_>>()
        + size_of::<CatalogHeader>()
        + 2 * size_of::<Result<CatalogRecordView<'_>>>()
        + crate::storage::semantic_catalog::schema_source_bytes()
}

/// Same record decoder as the full entry, without unrelated WAL layout parse.
pub(super) fn resident_decode_source_bytes() -> usize {
    use std::mem::size_of;
    2 * size_of::<CatalogRecordView<'_>>()
        + size_of::<CatalogHeader>()
        + 2 * size_of::<Result<CatalogRecordView<'_>>>()
        + crate::storage::semantic_catalog::catalog_read_codec_bytes()
}
