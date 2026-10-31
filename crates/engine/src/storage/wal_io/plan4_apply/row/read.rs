//! Row read operation: the original Owner is reborrowed for one source phase at
//! a time. Candidates carry observations, never a pin or a source slice.
use super::super::lookup_load::RetentionLoadPort as _;
use super::super::source::{OperationClass, OperationSourceAdmission};
use super::super::visibility::TableSlot;
use super::super::visibility::{ReferenceLease, TxnPin, UndoPin};
use super::*;
use crate::storage::{
    anchor::{self, TreeKind},
    catalog_record::{self, CatalogDefinitionView, CatalogExpectation},
    page::PageKind,
    tree_page::{self, TreeExpectation},
};
use crate::storage::{
    dirty_frame::ReadTargets,
    row_record::{RecordExpectation, RecordVersion},
    transaction_record::TransactionState,
    tree_page::TreePageView,
};
use crate::transaction::concurrency::ReadOperationAdmission;

/// Source-issued Txn and U pins of the last capture; held by the operation.
struct CapturedRowRefs<'b, 'a, 'c, 's> {
    transaction: Option<TxnPin<'b, 'a, 'c, 's>>,
    undo: Option<UndoPin<'b, 'a, 'c, 's>>,
}

pub(crate) struct RowReadOperation<
    'u,
    'r,
    't,
    'v,
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
    owner: &'u mut Owner<'c, 's>,
    statement: &'r RowStatementView<'t, 'v, 'a, 'c, 's>,
    loader: &'i mut [u8; 409_600],
    copy: &'i mut [u8; PAGE_BYTES],
    // Field order is the implicit drop order and matches `finish`: captured
    // refs before their lease, then source bytes before reader progress.
    captured: Option<CapturedRowRefs<'b, 'a, 'c, 's>>,
    lease: Option<ReferenceLease<'b, 'a, 'c, 's>>,
    capture_sequence: u64,
    stopped: bool,
    schema_len: usize,
    table: Option<ObjectHandle>,
    source: OperationSourceAdmission<'b, 'a, 'c, 's>,
    progress: ReadOperationAdmission<'c, 's>,
}

/// The validated anchor/leaf/allocation set. Its guards reborrow the Owner for
/// `'g` only; the other fields are disjoint borrows of the operation.
pub(crate) struct RowSearchPage<'g, 'b, 'a, 's, 'c, 'p, 'd> {
    source: ReadTargets<'g, 'b, 'c, 's, 'p, 'd, 3>,
    lease: &'g mut ReferenceLease<'b, 'a, 'c, 's>,
    captured: &'g mut Option<CapturedRowRefs<'b, 'a, 'c, 's>>,
    copy: &'g mut [u8; PAGE_BYTES],
    capture_sequence: &'g mut u64,
    expectation: RecordExpectation,
    schema_len: usize,
    table: ObjectHandle,
}

/// Lease generation, capture sequence and the observed row; not Copy.
pub(crate) struct RowCandidate<'b> {
    // Keep the original visibility binding borrowed even after operation
    // finish. Rebinding its backing resets generations and must not permit
    // an old observation to cross that open boundary.
    binding: std::marker::PhantomData<&'b ()>,
    lease_identity: (usize, usize, u64),
    capture_sequence: u64,
    version: RecordVersion,
    canonical_len: usize,
}
impl RowCandidate<'_> {
    pub(in crate::storage) fn row(&self) -> RowId {
        self.version.row
    }
    pub(in crate::storage) fn version(&self) -> RecordVersion {
        self.version
    }
    pub(in crate::storage) fn canonical_len(&self) -> usize {
        self.canonical_len
    }
}

/// C has checked the same U's Missing, row identity and creator/sequence.
pub(crate) struct MissingRowEvidence {
    _private: (),
}

/// Bytes copied from the actual capture into the caller output.
pub(crate) struct RowCopyObservation {
    version: RecordVersion,
    len: usize,
}
impl RowCopyObservation {
    pub(in crate::storage) fn version(&self) -> RecordVersion {
        self.version
    }
    pub(in crate::storage) fn len(&self) -> usize {
        self.len
    }
}

fn progress_error(error: crate::transaction::concurrency::ConcurrencyError) -> RetentionError {
    RetentionError {
        phase: RetentionPhase::Census,
        effect: error.effect,
        cause: RetentionCause::Concurrency(error),
    }
}
fn visibility_error(error: super::super::visibility::VisibilityError) -> RetentionError {
    RetentionError {
        phase: RetentionPhase::Census,
        effect: error.effect,
        cause: RetentionCause::Visibility(error),
    }
}
fn release_refs(refs: CapturedRowRefs<'_, '_, '_, '_>) -> Result<()> {
    let tx = refs.transaction.map_or(Ok(()), |pin| {
        pin.release_checked().map_err(visibility_error)
    });
    let undo = refs.undo.map_or(Ok(()), |pin| {
        pin.release_checked().map_err(visibility_error)
    });
    merge(tx, undo)
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    ReadPort<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    /// Reserve original reader progress and the Full source budget before
    /// touching the reference registry. The private loan remains caller-owned.
    pub(in crate::storage) fn row_operation<'u, 'r, 't, 'v, 'i>(
        &'r self,
        owner: &'u mut Owner<'c, 's>,
        view: &'r RowStatementView<'t, 'v, 'a, 'c, 's>,
        private: &'i mut [u8; 425_984],
    ) -> Result<RowReadOperation<'u, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>> {
        self.storage.health().map_err(|e| {
            fatal(
                self.storage,
                RetentionPhase::Prepare,
                RetentionCause::Storage(e),
            )
        })?;
        self.pool
            .check_retention_entry_health()
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
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
        let snapshot = view.catalog_snapshot(self.isolation, self.visibility)?;
        let visibility = self
            .visibility
            .ok_or_else(|| unchanged(RetentionPhase::Census, RetentionCause::WrongState))?;
        if !snapshot
            .confirms_origin(visibility)
            .map_err(visibility_error)?
        {
            return Err(unchanged(
                RetentionPhase::Census,
                RetentionCause::BindingMismatch,
            ));
        }
        let progress = owner
            .try_read_operation(self.pool.bound_core())
            .map_err(progress_error)?;
        let mut source = match OperationSourceAdmission::catalog(
            self.isolation,
            &self.source_charges,
            OperationClass::RowRead,
        ) {
            Ok(source) => source,
            Err(error) => {
                return merge(
                    Err(error.into()),
                    progress.finish_checked().map_err(progress_error),
                );
            }
        };
        let lease = (|| {
            health(self.storage, self.pool)?;
            self.pool
                .check_retention_reader(owner)
                .map_err(|e| frame(RetentionPhase::Census, e))?;
            if !snapshot.confirms(visibility).map_err(visibility_error)? {
                return Err(unchanged(
                    RetentionPhase::Census,
                    RetentionCause::WrongState,
                ));
            }
            view.check_catalog_read()?;
            visibility
                .reserve_references(owner)
                .map_err(visibility_error)
        })();
        let lease = match lease {
            Ok(lease) => lease,
            Err(error) => {
                let returned = source.release_checked().map_err(RetentionError::from);
                return merge(
                    merge(Err(error), returned),
                    progress.finish_checked().map_err(progress_error),
                );
            }
        };
        let (loader, copy) = private.split_at_mut(409_600);
        Ok(RowReadOperation {
            backend: self,
            owner,
            statement: view,
            loader: loader.try_into().expect("fixed loader partition"),
            copy: copy.try_into().expect("fixed copy partition"),
            captured: None,
            lease: Some(lease),
            capture_sequence: 0,
            stopped: false,
            schema_len: 0,
            table: None,
            progress,
            source,
        })
    }
}

impl<'u, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    RowReadOperation<'u, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    #[inline(never)]
    pub(in crate::storage) fn page<'g>(
        &'g mut self,
        table: &CheckedTable<'c, 's>,
        residency: &mut Plan4ResidencyCursor,
    ) -> Result<ReadStep<RowSearchPage<'g, 'b, 'a, 's, 'c, 'p, 'd>>> {
        if let ReadStep::FlushRequired { target } = self.load_catalog(table, residency)? {
            return Ok(ReadStep::FlushRequired { target });
        }
        self.capture_schema(table)?;
        let leaf = match self.load_leaf(table, residency)? {
            ReadStep::Ready(leaf) => leaf,
            ReadStep::FlushRequired { target } => return Ok(ReadStep::FlushRequired { target }),
        };
        self.issue_page(table, leaf)
    }
    #[inline(never)]
    fn check_table(&self, table: &CheckedTable<'c, 's>) -> Result<()> {
        if self.stopped || self.captured.is_some() {
            return Err(unchanged(
                RetentionPhase::Census,
                RetentionCause::WrongState,
            ));
        }
        self.statement
            .catalog_snapshot(self.backend.isolation, self.backend.visibility)?;
        self.statement.check_catalog_read()?;
        let bad = || {
            fatal(
                self.backend.storage,
                RetentionPhase::Census,
                RetentionCause::Invariant,
            )
        };
        if !std::ptr::eq(table.core, self.backend.pool.bound_core())
            || !std::ptr::eq(table.binding, &self.backend.storage.binding)
            || !std::ptr::eq(table.fence, self.backend.storage.session_fence())
        {
            return Err(unchanged(
                RetentionPhase::Census,
                RetentionCause::BindingMismatch,
            ));
        }
        let visibility = self.backend.visibility.ok_or_else(bad)?;
        if !matches!(visibility.table_slot(table.table).map_err(visibility_error)?, TableSlot::Published(slot) if slot == table.slot)
        {
            return Err(bad());
        }
        Ok(())
    }
    #[inline(never)]
    fn catalog_request(&self, index: usize) -> Result<TargetRequest> {
        let header =
            super::super::super::genesis::owned_header(&self.backend.storage.core.plan, index)
                .map_err(|_| {
                    fatal(
                        self.backend.storage,
                        RetentionPhase::Census,
                        RetentionCause::Invariant,
                    )
                })?;
        Ok(TargetRequest {
            identity: header.identity,
            kind: header.kind,
            level: 0,
        })
    }
    #[inline(never)]
    fn catalog_requests(&self) -> Result<[TargetRequest; 2]> {
        Ok([self.catalog_request(7)?, self.catalog_request(8)?])
    }
    #[inline(never)]
    fn load_catalog(
        &mut self,
        table: &CheckedTable<'c, 's>,
        residency: &mut Plan4ResidencyCursor,
    ) -> Result<ReadStep<()>> {
        self.check_table(table)?;
        let requests = self.catalog_requests()?;
        for at in 0..requests.len() {
            if let RetentionResidencyStep::FlushRequired { target, .. } =
                self.backend.load_lookup_target(
                    self.owner,
                    requests[at],
                    residency,
                    &requests[..at],
                    self.loader,
                )?
            {
                self.stopped = true;
                return Ok(ReadStep::FlushRequired { target });
            }
        }
        Ok(ReadStep::Ready(()))
    }
    #[inline(never)]
    fn capture_schema(&mut self, table: &CheckedTable<'c, 's>) -> Result<()> {
        let bad = || {
            fatal(
                self.backend.storage,
                RetentionPhase::Census,
                RetentionCause::Invariant,
            )
        };
        let guard = self.backend.read_catalog(self.owner)?;
        let schema = (|| {
            if guard.has_table_indexes(table.table.object)? {
                return Err(unchanged(
                    RetentionPhase::Census,
                    RetentionCause::Unsupported,
                ));
            }
            let record = guard.lookup(table.table.object)?.ok_or_else(bad)?;
            let header = record.header();
            if header.identity != table.table
                || header.anchor != table.anchor.page
                || header.retired
                || !matches!(record.definition(), CatalogDefinitionView::Table(_))
            {
                return Err(bad());
            }
            let bytes = guard.value(table.table.object)?.ok_or_else(bad)?;
            if bytes.len() > 8192 {
                return Err(bad());
            }
            self.copy[..bytes.len()].copy_from_slice(bytes);
            Ok(bytes.len())
        })();
        self.schema_len = merge(schema, guard.release_checked())?;
        self.table = Some(table.table);
        Ok(())
    }
    #[inline(never)]
    fn row_leaf_request(
        &mut self,
        table: &CheckedTable<'c, 's>,
        anchor_request: TargetRequest,
    ) -> Result<TargetRequest> {
        let bad = || {
            fatal(
                self.backend.storage,
                RetentionPhase::Census,
                RetentionCause::Invariant,
            )
        };
        let allocator = self
            .backend
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Source, e))?;
        let anchor_guard = self
            .backend
            .pool
            .reserve_target_set(self.owner, &[allocator, anchor_request])
            .and_then(|set| set.try_read())
            .map_err(|e| frame(RetentionPhase::Source, e))?;
        let root = (|| {
            super::super::read::membership(
                self.backend.storage,
                allocator,
                anchor_guard
                    .bytes(0)
                    .map_err(|e| frame(RetentionPhase::Source, e))?,
                anchor_request,
            )?;
            anchor::decode_anchor(
                anchor_guard
                    .bytes(1)
                    .map_err(|e| frame(RetentionPhase::Source, e))?,
                table.anchor,
            )
            .map(|a| a.root())
            .map_err(|e| {
                fatal(
                    self.backend.storage,
                    RetentionPhase::Source,
                    RetentionCause::Page(e),
                )
            })
        })();
        let root = merge(
            root,
            anchor_guard
                .release_checked()
                .map_err(|e| frame(RetentionPhase::Source, e)),
        )?;
        if root.tree != TreeKind::Row || root.level != 0 || root.revision != 1 {
            return Err(bad());
        }
        Ok(TargetRequest {
            identity: PageIdentity {
                page: root.page,
                ..table.anchor
            },
            kind: PageKind::RowLeaf,
            level: 0,
        })
    }
    #[inline(never)]
    fn load_leaf(
        &mut self,
        table: &CheckedTable<'c, 's>,
        residency: &mut Plan4ResidencyCursor,
    ) -> Result<ReadStep<TargetRequest>> {
        let anchor_request = TargetRequest {
            identity: table.anchor,
            kind: PageKind::TreeAnchor,
            level: 0,
        };
        if let RetentionResidencyStep::FlushRequired { target, .. } = self
            .backend
            .load_lookup_target(self.owner, anchor_request, residency, &[], self.loader)?
        {
            self.stopped = true;
            return Ok(ReadStep::FlushRequired { target });
        }
        let leaf = self.row_leaf_request(table, anchor_request)?;
        match self.backend.load_lookup_target(
            self.owner,
            leaf,
            residency,
            std::slice::from_ref(&anchor_request),
            self.loader,
        )? {
            RetentionResidencyStep::FlushRequired { target, .. } => {
                self.stopped = true;
                return Ok(ReadStep::FlushRequired { target });
            }
            RetentionResidencyStep::Resident { .. } => {}
        }
        Ok(ReadStep::Ready(leaf))
    }
    #[inline(never)]
    fn issue_page<'g>(
        &'g mut self,
        table: &CheckedTable<'c, 's>,
        leaf: TargetRequest,
    ) -> Result<ReadStep<RowSearchPage<'g, 'b, 'a, 's, 'c, 'p, 'd>>> {
        let bad = || {
            fatal(
                self.backend.storage,
                RetentionPhase::Census,
                RetentionCause::Invariant,
            )
        };
        let allocator = self
            .backend
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Source, e))?;
        let anchor_request = TargetRequest {
            identity: table.anchor,
            kind: PageKind::TreeAnchor,
            level: 0,
        };
        self.statement.record_table_read(table.slot)?;
        let source = self
            .backend
            .pool
            .reserve_target_set(&mut *self.owner, &[allocator, anchor_request, leaf])
            .and_then(|set| set.try_read())
            .map_err(|e| frame(RetentionPhase::Source, e))?;
        let checked = (|| {
            for (at, request) in [(1, anchor_request), (2, leaf)] {
                super::super::read::membership(
                    self.backend.storage,
                    allocator,
                    source
                        .bytes(0)
                        .map_err(|e| frame(RetentionPhase::Source, e))?,
                    request,
                )?;
                let observed = source
                    .observe(at)
                    .map_err(|e| frame(RetentionPhase::Source, e))?;
                let envelope = crate::storage::page::decode_page(
                    source
                        .bytes(at)
                        .map_err(|e| frame(RetentionPhase::Source, e))?,
                    request.identity,
                    request.kind,
                )
                .map_err(|e| {
                    fatal(
                        self.backend.storage,
                        RetentionPhase::Source,
                        RetentionCause::Page(e),
                    )
                })?;
                if envelope.header().lsn != observed.frame.page_lsn {
                    return Err(bad());
                }
            }
            let current = anchor::decode_anchor(
                source
                    .bytes(1)
                    .map_err(|e| frame(RetentionPhase::Source, e))?,
                table.anchor,
            )
            .map_err(|e| {
                fatal(
                    self.backend.storage,
                    RetentionPhase::Source,
                    RetentionCause::Page(e),
                )
            })?;
            if current.root().tree != TreeKind::Row
                || current.root().page != leaf.identity.page
                || current.root().level != 0
                || current.root().revision != 1
            {
                return Err(bad());
            }
            current
                .decode_root(
                    source
                        .bytes(2)
                        .map_err(|e| frame(RetentionPhase::Source, e))?,
                )
                .map_err(|e| {
                    fatal(
                        self.backend.storage,
                        RetentionPhase::Source,
                        RetentionCause::Page(e),
                    )
                })?;
            Ok(())
        })();
        if let Err(error) = checked {
            return merge(
                Err(error),
                source
                    .release_checked()
                    .map_err(|e| frame(RetentionPhase::Source, e)),
            );
        }
        Ok(ReadStep::Ready(RowSearchPage {
            source,
            lease: self.lease.as_mut().ok_or_else(bad)?,
            captured: &mut self.captured,
            copy: self.copy,
            capture_sequence: &mut self.capture_sequence,
            expectation: RecordExpectation {
                namespace: self.backend.storage.profile().spec().namespace,
                row: RowId::new(1).expect("nonzero"),
            },
            schema_len: self.schema_len,
            table: table.table,
        }))
    }
    #[inline(never)]
    pub(in crate::storage) fn status(
        &mut self,
        candidate: &RowCandidate<'b>,
        residency: &mut Plan4ResidencyCursor,
    ) -> Result<ReadStep<TransactionState>> {
        use crate::storage::transaction_record::VersionStamp;
        self.check_candidate(candidate)?;
        #[cfg(test)]
        crate::transaction::concurrency::catalog_test_support::hit(
            self.backend.pool.bound_core(),
            crate::transaction::concurrency::catalog_test_support::Point::RowReadCaptured,
        );
        let bad = || {
            fatal(
                self.backend.storage,
                RetentionPhase::Census,
                RetentionCause::Invariant,
            )
        };
        let VersionStamp::Transaction(reference) = candidate.version.stamp else {
            return Err(bad());
        };
        if self
            .captured
            .as_ref()
            .and_then(|r| r.transaction.as_ref())
            .map(|p| p.reference())
            != Some(reference)
        {
            return Err(bad());
        }
        let (request, location) = self
            .backend
            .catalog_status_location(self.owner, reference)?;
        if let RetentionResidencyStep::FlushRequired { target, .. } = self
            .backend
            .load_lookup_target(self.owner, request, residency, &[], self.loader)?
        {
            self.stopped = true;
            return Ok(ReadStep::FlushRequired { target });
        }
        self.read_status(request, location, reference)
    }
    #[inline(never)]
    fn read_status(
        &mut self,
        request: TargetRequest,
        location: crate::storage::transaction_record::StatusSlotRef,
        reference: crate::storage::transaction_record::TxnRef,
    ) -> Result<ReadStep<TransactionState>> {
        use crate::storage::transaction_record::TransactionStatusCell;
        let bad = || {
            fatal(
                self.backend.storage,
                RetentionPhase::Census,
                RetentionCause::Invariant,
            )
        };
        let guard = self.backend.read(self.owner, &Plan4Target { request })?;
        let result = (|| {
            let page = crate::storage::status_home::decode_status_home_page(
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
                return Err(bad());
            };
            if status.reference() != reference {
                return Err(bad());
            }
            Ok(status.state())
        })();
        let state = merge(result, guard.release_checked())?;
        self.statement.check_catalog_read()?;
        Ok(ReadStep::Ready(state))
    }
    #[inline(never)]
    pub(in crate::storage) fn missing_before(
        &mut self,
        candidate: &RowCandidate<'b>,
        residency: &mut Plan4ResidencyCursor,
    ) -> Result<ReadStep<MissingRowEvidence>> {
        self.check_candidate(candidate)?;
        let bad = || {
            fatal(
                self.backend.storage,
                RetentionPhase::Census,
                RetentionCause::Invariant,
            )
        };
        let reference = candidate.version.undo.ok_or_else(bad)?;
        if self
            .captured
            .as_ref()
            .and_then(|r| r.undo.as_ref())
            .map(|p| p.reference())
            != Some(reference)
        {
            return Err(bad());
        }
        let table = self.table.ok_or_else(bad)?;
        let request = self.backend.catalog_undo_location(self.owner, reference)?;
        if let RetentionResidencyStep::FlushRequired { target, .. } = self
            .backend
            .load_lookup_target(self.owner, request, residency, &[], self.loader)?
        {
            self.stopped = true;
            return Ok(ReadStep::FlushRequired { target });
        }
        self.read_missing(candidate, reference, table, request)
    }
    #[inline(never)]
    fn read_missing(
        &mut self,
        candidate: &RowCandidate<'b>,
        reference: crate::storage::undo_ref::UndoRef,
        table: ObjectHandle,
        request: TargetRequest,
    ) -> Result<ReadStep<MissingRowEvidence>> {
        use crate::storage::{
            transaction_record::VersionStamp,
            undo_record::{
                self, UndoBeforeKind, UndoPayload, UndoRecordExpectation, UndoTargetKind,
                UndoTargetView,
            },
        };
        let bad = || {
            fatal(
                self.backend.storage,
                RetentionPhase::Census,
                RetentionCause::Invariant,
            )
        };
        let guard = self
            .backend
            .read_catalog_undo(self.owner, &Plan4Target { request })?;
        let result = (|| {
            let page =
                crate::storage::page::decode_page(guard.bytes()?, request.identity, PageKind::Undo)
                    .map_err(|e| {
                        fatal(
                            self.backend.storage,
                            RetentionPhase::Census,
                            RetentionCause::Page(e),
                        )
                    })?;
            let cell = page.cell(usize::from(reference.slot())).ok_or_else(bad)?;
            let expected = UndoRecordExpectation {
                namespace: self.backend.storage.profile().spec().namespace,
                reference,
                target: UndoTargetView {
                    kind: UndoTargetKind::Row,
                    owner: table.object,
                    generation: table.generation,
                    entity: candidate.version.row.get(),
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
            if record.creator.id != candidate.version.logical.transaction()
                || matches!(candidate.version.stamp,VersionStamp::Transaction(actor) if actor.id()!=record.creator.id || actor.generation()!=record.creator.status_generation)
                || record.mutation_sequence != candidate.version.logical.mutation_sequence()
                || record.sequence == 0
                || record.before_kind != UndoBeforeKind::Missing
                || record.before_stamp.is_some()
                || record.before_logical.is_some()
                || !matches!(record.payload, UndoPayload::None)
            {
                return Err(bad());
            }
            Ok(MissingRowEvidence { _private: () })
        })();
        merge(result, guard.release_checked()).map(ReadStep::Ready)
    }
    fn check_candidate(&self, candidate: &RowCandidate<'b>) -> Result<()> {
        if self.stopped
            || self.lease.as_ref().map(|l| l.row_identity()) != Some(candidate.lease_identity)
            || candidate.capture_sequence != self.capture_sequence
            || self.captured.is_none()
        {
            return Err(unchanged(
                RetentionPhase::Census,
                RetentionCause::WrongState,
            ));
        }
        self.statement.check_catalog_read()
    }
    /// The visible decision stays with B; this only copies the actual capture.
    pub(in crate::storage) fn copy_candidate(
        &mut self,
        candidate: &RowCandidate<'b>,
        output: &mut [u8],
    ) -> Result<RowCopyObservation> {
        self.check_candidate(candidate)?;
        if output.len() < candidate.canonical_len {
            return Err(unchanged(RetentionPhase::Census, RetentionCause::Capacity));
        }
        output[..candidate.canonical_len]
            .copy_from_slice(&self.copy[8192..8192 + candidate.canonical_len]);
        Ok(RowCopyObservation {
            version: candidate.version,
            len: candidate.canonical_len,
        })
    }
    pub(in crate::storage) fn release_candidate(
        &mut self,
        candidate: RowCandidate<'b>,
    ) -> Result<()> {
        // FlushRequired stops new source work but does not prevent cleanup.
        let stopped = self.stopped;
        self.stopped = false;
        let check = self.check_candidate(&candidate);
        self.stopped = stopped;
        check?;
        self.captured.take().map_or(Ok(()), release_refs)
    }
    /// Checked cleanup in source order: captured refs, lease, then admission.
    pub(in crate::storage) fn finish(mut self) -> Result<()> {
        self.stopped = true;
        let refs = self.captured.take().map_or(Ok(()), release_refs);
        let lease = self.lease.take().map_or(Ok(()), |lease| {
            lease.release_checked().map_err(visibility_error)
        });
        let source = self.source.release_checked().map_err(RetentionError::from);
        let progress = self.progress.finish_checked().map_err(progress_error);
        merge(merge(merge(refs, lease), source), progress)
    }
}

impl<'g, 'b, 'a, 's, 'c, 'p, 'd> RowSearchPage<'g, 'b, 'a, 's, 'c, 'p, 'd> {
    pub(in crate::storage) fn leaf(&self) -> Result<TreePageView<'_>> {
        let request = self
            .source
            .target_request(2)
            .map_err(|e| frame(RetentionPhase::Source, e))?;
        tree_page::decode_tree_page(
            self.source
                .bytes(2)
                .map_err(|e| frame(RetentionPhase::Source, e))?,
            TreeExpectation {
                identity: request.identity,
                kind: PageKind::RowLeaf,
                level: 0,
                arity: 0,
            },
        )
        .map_err(|e| self.lease.row_source_failure(RetentionCause::Tree(e)))
    }
    /// Checks the slot chosen by B against the latched row before any pin.
    pub(in crate::storage) fn capture(self, at: usize) -> Result<RowCandidate<'b>> {
        let result = (|| {
            if self.captured.is_some() {
                return Err(unchanged(
                    RetentionPhase::Census,
                    RetentionCause::WrongState,
                ));
            }
            let sequence = self
                .capture_sequence
                .checked_add(1)
                .ok_or_else(|| unchanged(RetentionPhase::Census, RetentionCause::Capacity))?;
            let (schema, copy) = self.copy.split_at_mut(8192);
            let record = catalog_record::decode_catalog_record(
                &schema[..self.schema_len],
                CatalogExpectation {
                    namespace: self.expectation.namespace,
                    object: self.table.object,
                },
            )
            .map_err(|e| unchanged(RetentionPhase::Census, RetentionCause::Catalog(e)))?;
            let CatalogDefinitionView::Table(schema) = record.definition() else {
                return Err(unchanged(RetentionPhase::Census, RetentionCause::Invariant));
            };
            let (refs, record) = self.lease.capture_row(
                &self.source,
                at,
                self.expectation.namespace,
                crate::storage::row_record::RowSchemaInput::Catalog(schema),
            )?;
            let version = record.version();
            let crate::storage::row_record::RowPayload::Inline(value) = record.payload() else {
                unreachable!("capture validated Inline");
            };
            let len = value.as_bytes().len();
            copy[..len].copy_from_slice(value.as_bytes());
            *self.captured = Some(CapturedRowRefs {
                transaction: refs.transaction,
                undo: refs.undo,
            });
            *self.capture_sequence = sequence;
            Ok(RowCandidate {
                binding: std::marker::PhantomData,
                lease_identity: self.lease.row_identity(),
                capture_sequence: sequence,
                version,
                canonical_len: len,
            })
        })();
        merge(result, self.release_checked())
    }
    pub(in crate::storage) fn release_checked(self) -> Result<()> {
        self.source
            .release_checked()
            .map_err(|e| frame(RetentionPhase::Source, e))
    }
}

#[cfg(test)]
impl<W: WriterNamespace> ReadPort<'_, '_, '_, '_, '_, '_, '_, '_, W> {
    /// Raw size observations, not an acceptance assertion or a replacement for
    /// the phase census. Loader and operation necessarily coexist at LOAD.
    pub(in crate::storage) fn row_test_load_inventory() -> [usize; 11] {
        use std::mem::size_of as n;
        let r = n::<&()>();
        let page = 3 * r
            + n::<Result<()>>()
            + n::<Result<ReadStep<()>>>()
            + n::<Result<ReadStep<TargetRequest>>>()
            + n::<Result<ReadStep<RowSearchPage<'_, '_, '_, '_, '_, '_, '_>>>>();
        let catalog = 3 * r
            + n::<Result<()>>()
            + n::<Result<[TargetRequest; 2]>>()
            + n::<std::ops::Range<usize>>()
            + n::<usize>()
            + n::<Result<RetentionResidencyStep>>();
        let leaf = 3 * r
            + n::<TargetRequest>()
            + n::<Result<RetentionResidencyStep>>()
            + n::<Result<TargetRequest>>()
            + n::<Result<RetentionResidencyStep>>();
        [
            page,
            catalog,
            leaf,
            n::<TargetRequest>(),
            n::<Result<RetentionResidencyStep>>(),
            n::<Result<[TargetRequest; 2]>>(),
            n::<RowReadOperation<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>>(),
            super::super::lookup_load::source_bytes::<W>(),
            super::super::source::CatalogSourceCharges::new::<W>().observations()[1],
            // status: args/check/closure, stamp and pin comparison, location,
            // LOAD and the C return carrier. Decode is a separate callee.
            4 * r
                + n::<Result<()>>()
                + n::<crate::storage::transaction_record::VersionStamp>()
                + 2 * n::<Option<&()>>()
                + 2 * n::<Option<crate::storage::transaction_record::TxnRef>>()
                + n::<
                    Result<(
                        TargetRequest,
                        crate::storage::transaction_record::StatusSlotRef,
                    )>,
                >()
                + n::<Result<RetentionResidencyStep>>()
                + n::<Result<ReadStep<TransactionState>>>(),
            // missing: original candidate/pins and table/request observations.
            4 * r
                + n::<Result<()>>()
                + n::<Result<UndoRef>>()
                + 2 * n::<Option<&()>>()
                + 2 * n::<Option<UndoRef>>()
                + n::<Result<ObjectHandle>>()
                + n::<Result<TargetRequest>>()
                + n::<Result<RetentionResidencyStep>>()
                + n::<Result<ReadStep<MissingRowEvidence>>>(),
        ]
    }
    pub(in crate::storage) fn row_test_with_retiring_undo(
        &self,
        undo: UndoRef,
        run: impl FnOnce(),
    ) {
        self.visibility
            .expect("actual visibility binding")
            .test_with_retiring_undo(undo, run);
    }
    pub(in crate::storage) fn row_test_source_observations(&self) -> [usize; 5] {
        [
            self.source_charges.observations()[1],
            super::super::lookup_load::source_bytes::<W>(),
            std::mem::size_of::<
                RowReadOperation<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>,
            >(),
            std::mem::size_of::<RowSearchPage<'_, '_, '_, '_, '_, '_, '_>>(),
            std::mem::size_of::<RowCandidate>(),
        ]
    }
}

#[cfg(test)]
impl RowCandidate<'_> {
    pub(in crate::storage) fn test_duplicate(&self) -> Self {
        Self {
            binding: std::marker::PhantomData,
            lease_identity: self.lease_identity,
            capture_sequence: self.capture_sequence,
            version: self.version,
            canonical_len: self.canonical_len,
        }
    }
}
#[cfg(test)]
impl<W: WriterNamespace> RowReadOperation<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W> {
    /// Test setup through the actual checked LOAD, with no authority fabrication.
    pub(in crate::storage) fn test_load(
        &mut self,
        target: &Plan4Target,
    ) -> Result<RetentionResidencyStep> {
        self.backend.load_lookup_target(
            self.owner,
            target.request,
            &mut Plan4ResidencyCursor::new(),
            &[],
            self.loader,
        )
    }
    pub(in crate::storage) fn test_set_sequence(&mut self, sequence: u64) {
        self.capture_sequence = sequence;
    }
}

#[cfg(test)]
#[path = "read/census.rs"]
mod census;
#[cfg(test)]
impl<W: WriterNamespace> ReadPort<'_, '_, '_, '_, '_, '_, '_, '_, W> {
    pub(in crate::storage) fn row_test_nonload_inventory() -> [usize; 7] {
        census::nonload::<W>()
    }
}

#[cfg(test)]
impl<'s, 'c, W: WriterNamespace> Plan4Retention<'_, 's, 'c, '_, '_, '_, '_, '_, W> {
    /// Fixture-only observation of an already resident original target. This
    /// grants no install/load permission; normal eviction revalidates it.
    pub(in crate::storage) fn row_test_resident_target(&self, index: usize) -> Result<Plan4Target> {
        self.inner
            .pool
            .target_request(index)
            .map(|request| Plan4Target { request })
            .map_err(|e| frame(RetentionPhase::Source, e))
    }
}
