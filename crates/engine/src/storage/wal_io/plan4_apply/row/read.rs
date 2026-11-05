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
    /// The capture whose U `missing_before` reported as a value before; only
    /// that Inline candidate may ask `value_before` (0: none).
    value_capture: u64,
    stopped: bool,
    /// The last issued leaf's exclusive upper key bound from its parents:
    /// the first key of the next leaf, None for the rightmost leaf
    /// (dev3-tree-smo.md#current-leaf-consumers).
    upper: Option<RowId>,
    schema_len: usize,
    table: Option<ObjectHandle>,
    source: OperationSourceAdmission<'b, 'a, 'c, 's>,
    progress: ReadOperationAdmission<'c, 's>,
}

/// One page step's route (dev3-tree-smo.md#current-leaf-consumers): the
/// table's current RowLeaf for the step's key, the anchor revision it was
/// routed at and the key bounds its parents give it (none for a level0 root).
#[derive(Clone, Copy)]
struct LeafRoute {
    leaf: crate::storage::page::PageRef,
    revision: u64,
    bounds: tree_page::search::RowRouteBounds,
}

/// A walk of the key's path over resident pages: the RowLeaf it reaches, or
/// the first RowInternal page (page, level) of the path that is not resident.
enum RouteNext {
    Leaf(LeafRoute),
    Missing(crate::storage::page::PageRef, u8),
}

/// The LOAD steps one route may take before it refuses: a path's pages are
/// made resident top-down, and only a page evicted again by a later LOAD of
/// the same walk is loaded twice.
const ROUTE_LOAD_STEPS: u8 = 32;

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
    deleted: bool,
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
    /// A DELETE tombstone (no canonical value; `canonical_len` is 0).
    pub(in crate::storage) fn deleted(&self) -> bool {
        self.deleted
    }
}

/// The tombstone's U RowValue before (dev3-row-delete.md#read): the
/// committed row it replaced (`q_old`, its logical version, no undo), whose
/// canonical value is now in the operation's copy buffer. B decides with
/// `q_old`; only then may it copy.
pub(crate) struct RowValueBefore<'b> {
    binding: std::marker::PhantomData<&'b ()>,
    lease_identity: (usize, usize, u64),
    capture_sequence: u64,
    row: RowId,
    q_old: foundation::identity::CommitSeq,
    logical: crate::storage::transaction_record::LogicalRowVersion,
    canonical_len: usize,
}
impl RowValueBefore<'_> {
    pub(in crate::storage) fn row(&self) -> RowId {
        self.row
    }
    pub(in crate::storage) fn q_old(&self) -> u64 {
        self.q_old.get()
    }
    pub(in crate::storage) fn canonical_len(&self) -> usize {
        self.canonical_len
    }
}

/// C has checked the same U's row identity and creator/sequence, and
/// either its Missing before (an INSERT's U: the row did not exist) or its
/// committed Inline RowValue before (an UPDATE's U, dev3-row-update.md#read:
/// the old value exists and `value_before` may now read it for this capture).
pub(crate) struct MissingRowEvidence {
    value: bool,
}
impl MissingRowEvidence {
    /// The U holds a committed Inline before value (`ValueBefore`).
    pub(in crate::storage) fn value_before(&self) -> bool {
        self.value
    }
}

/// The operation's copy buffer holds the schema in `[0, 8192)`, the
/// captured value in `[8192, 10240)` and a U's before value in
/// `[10240, 12288)` (dev3-row-update.md#read), so neither copy overwrites
/// the other.
const VALUE_COPY: usize = 8192;
const BEFORE_COPY: usize = 10240;
const _: () = assert!(VALUE_COPY + 2048 <= BEFORE_COPY && BEFORE_COPY + 2048 <= PAGE_BYTES);

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
            value_capture: 0,
            stopped: false,
            upper: None,
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
    /// The table's current RowLeaf that routes `key`
    /// (dev3-tree-smo.md#current-leaf-consumers): the anchor's level0 root,
    /// or the leaf reached through its RowInternal pages. Its upper key bound
    /// (`upper`) lets a scan continue in the next leaf.
    pub(in crate::storage) fn page<'g>(
        &'g mut self,
        table: &CheckedTable<'c, 's>,
        residency: &mut Plan4ResidencyCursor,
        key: RowId,
    ) -> Result<ReadStep<RowSearchPage<'g, 'b, 'a, 's, 'c, 'p, 'd>>> {
        if let ReadStep::FlushRequired { target } = self.load_catalog(table, residency)? {
            return Ok(ReadStep::FlushRequired { target });
        }
        self.capture_schema(table)?;
        let route = match self.load_leaf(table, residency, key)? {
            ReadStep::Ready(route) => route,
            ReadStep::FlushRequired { target } => return Ok(ReadStep::FlushRequired { target }),
        };
        self.issue_page(table, route, key)
    }
    /// The upper key bound of the leaf `page` last issued.
    pub(in crate::storage) fn upper(&self) -> Option<RowId> {
        self.upper
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
    /// The anchor's current root under a brief read with its membership.
    #[inline(never)]
    fn anchor_root(
        &mut self,
        table: &CheckedTable<'c, 's>,
        anchor_request: TargetRequest,
    ) -> Result<anchor::AnchorRoot> {
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
        if root.tree != TreeKind::Row {
            return Err(bad());
        }
        Ok(root)
    }
    /// One route step under a brief read of the RowInternal page `internal`
    /// with its membership: its keys lie in `bounds` and it names the child
    /// that routes `key`, one level down in the same tree. None: the page is
    /// not resident (the pool's unchanged MissingTarget), nothing was read.
    #[inline(never)]
    fn route_internal(
        &mut self,
        internal: TargetRequest,
        key: &[u8; 8],
        bounds: &mut tree_page::search::RowRouteBounds,
    ) -> Result<Option<TreeExpectation>> {
        let storage = self.backend.storage;
        let allocator = self
            .backend
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Source, e))?;
        let guard = match self
            .backend
            .pool
            .reserve_target_set(self.owner, &[allocator, internal])
            .and_then(|set| set.try_read())
        {
            Ok(guard) => guard,
            Err(crate::storage::dirty_frame::DirtyError {
                cause: crate::storage::dirty_frame::Cause::MissingTarget,
                effect: foundation::error::Effect::Unchanged,
            }) => return Ok(None),
            Err(e) => return Err(frame(RetentionPhase::Source, e)),
        };
        let child = (|| {
            super::super::read::membership(
                storage,
                allocator,
                guard
                    .bytes(0)
                    .map_err(|e| frame(RetentionPhase::Source, e))?,
                internal,
            )?;
            let child = tree_page::search::route_row_step(
                guard
                    .bytes(1)
                    .map_err(|e| frame(RetentionPhase::Source, e))?,
                TreeExpectation {
                    identity: internal.identity,
                    kind: PageKind::RowInternal,
                    level: internal.level,
                    arity: 0,
                },
                key,
                bounds,
            )
            .map_err(|e| fatal(storage, RetentionPhase::Source, RetentionCause::Tree(e)))?;
            match child {
                Some(child)
                    if child.level + 1 == internal.level
                        && child.identity
                            == (PageIdentity {
                                page: child.identity.page,
                                ..internal.identity
                            }) =>
                {
                    Ok(Some(child))
                }
                _ => Err(fatal(
                    storage,
                    RetentionPhase::Census,
                    RetentionCause::Invariant,
                )),
            }
        })();
        merge(
            child,
            guard
                .release_checked()
                .map_err(|e| frame(RetentionPhase::Source, e)),
        )
    }
    /// `key`'s path from the anchor's current root over resident pages,
    /// each under its own brief read: the RowLeaf it reaches with the bounds
    /// its parents give it, or the first RowInternal that is not resident.
    #[inline(never)]
    fn route_resident(
        &mut self,
        table: &CheckedTable<'c, 's>,
        anchor_request: TargetRequest,
        key: RowId,
    ) -> Result<RouteNext> {
        let root = self.anchor_root(table, anchor_request)?;
        let key = key.get().to_be_bytes();
        let mut route = LeafRoute {
            leaf: root.page,
            revision: root.revision,
            bounds: tree_page::search::RowRouteBounds::default(),
        };
        let mut level = root.level;
        while level != 0 {
            let internal = TargetRequest {
                identity: PageIdentity {
                    page: route.leaf,
                    ..table.anchor
                },
                kind: PageKind::RowInternal,
                level,
            };
            match self.route_internal(internal, &key, &mut route.bounds)? {
                Some(child) => {
                    route.leaf = child.identity.page;
                    level = child.level;
                }
                None => return Ok(RouteNext::Missing(route.leaf, level)),
            }
        }
        Ok(RouteNext::Leaf(route))
    }
    /// The anchor, then `key`'s route and the RowLeaf it reaches, each page
    /// through the one LOAD body protecting the anchor. The route is walked
    /// again over resident pages after each RowInternal LOAD, so no route
    /// value is held across a LOAD; levels strictly decrease, so a walk
    /// reads at most the root's level of internal pages.
    #[inline(never)]
    fn load_leaf(
        &mut self,
        table: &CheckedTable<'c, 's>,
        residency: &mut Plan4ResidencyCursor,
        key: RowId,
    ) -> Result<ReadStep<LeafRoute>> {
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
        let mut steps = 0u8;
        loop {
            let route = self.route_resident(table, anchor_request, key)?;
            if steps == ROUTE_LOAD_STEPS {
                return Err(unchanged(RetentionPhase::Load, RetentionCause::Busy));
            }
            steps += 1;
            // The request is the LOAD's own argument, built in place.
            match self.backend.load_lookup_target(
                self.owner,
                match route {
                    RouteNext::Leaf(LeafRoute { leaf, .. }) => TargetRequest {
                        identity: PageIdentity {
                            page: leaf,
                            ..table.anchor
                        },
                        kind: PageKind::RowLeaf,
                        level: 0,
                    },
                    RouteNext::Missing(page, level) => TargetRequest {
                        identity: PageIdentity {
                            page,
                            ..table.anchor
                        },
                        kind: PageKind::RowInternal,
                        level,
                    },
                },
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
            if let RouteNext::Leaf(route) = route {
                return Ok(ReadStep::Ready(route));
            }
        }
    }
    #[inline(never)]
    fn issue_page<'g>(
        &'g mut self,
        table: &CheckedTable<'c, 's>,
        route: LeafRoute,
        key: RowId,
    ) -> Result<ReadStep<RowSearchPage<'g, 'b, 'a, 's, 'c, 'p, 'd>>> {
        let leaf = TargetRequest {
            identity: PageIdentity {
                page: route.leaf,
                ..table.anchor
            },
            kind: PageKind::RowLeaf,
            level: 0,
        };
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
            // The anchor still has the root revision the route read: the
            // read operation's gate keeps a structural change out between
            // them, and every structural change advances the revision.
            let root = current.root();
            if root.tree != TreeKind::Row || root.revision != route.revision {
                return Err(bad());
            }
            let bytes = source
                .bytes(2)
                .map_err(|e| frame(RetentionPhase::Source, e))?;
            if root.level == 0 {
                if root.page != leaf.identity.page {
                    return Err(bad());
                }
                current.decode_root(bytes).map_err(|e| {
                    fatal(
                        self.backend.storage,
                        RetentionPhase::Source,
                        RetentionCause::Page(e),
                    )
                })?;
            } else {
                // A routed leaf's keys lie within its parents' bounds.
                let mut bounds = route.bounds;
                let next = tree_page::search::route_row_step(
                    bytes,
                    TreeExpectation {
                        identity: leaf.identity,
                        kind: PageKind::RowLeaf,
                        level: 0,
                        arity: 0,
                    },
                    &key.get().to_be_bytes(),
                    &mut bounds,
                )
                .map_err(|e| {
                    fatal(
                        self.backend.storage,
                        RetentionPhase::Source,
                        RetentionCause::Tree(e),
                    )
                })?;
                if next.is_some() {
                    return Err(bad());
                }
            }
            route
                .bounds
                .upper
                .map(|upper| RowId::new(u64::from_be_bytes(upper)).map_err(|_| bad()))
                .transpose()
        })();
        self.upper = match checked {
            Ok(upper) => upper,
            Err(error) => {
                return merge(
                    Err(error),
                    source
                        .release_checked()
                        .map_err(|e| frame(RetentionPhase::Source, e)),
                );
            }
        };
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
        test_capture_claim_probe(self.backend.visibility, self.owner);
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
        if candidate.deleted {
            return Err(unchanged(
                RetentionPhase::Census,
                RetentionCause::WrongState,
            ));
        }
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
    /// The U through entry (b) (dev3-row-update.md#read): its outline
    /// proves Missing (evidence), a committed Inline RowValue (a value
    /// before, read later by `value_before`), or an unsupported before;
    /// everything else is corruption.
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
                self, UndoBeforeKind, UndoPayloadForm, UndoRecordExpectation, UndoRecordState,
                UndoStampKind, UndoTargetKind, UndoTargetView,
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
            let outline = undo_record::lookup_undo_record_metadata(
                guard.bytes()?,
                request.identity,
                expected,
            )
            .map_err(|e| {
                fatal(
                    self.backend.storage,
                    RetentionPhase::Census,
                    RetentionCause::Undo(e),
                )
            })?;
            // Readers and Row writers exclude each other, so a captured
            // version's U is never released under the reader.
            if outline.state != UndoRecordState::Live {
                return Err(bad());
            }
            let record = outline.metadata;
            if record.creator.id != candidate.version.logical.transaction()
                || matches!(candidate.version.stamp,VersionStamp::Transaction(actor) if actor.id()!=record.creator.id || actor.generation()!=record.creator.status_generation)
                || record.mutation_sequence != candidate.version.logical.mutation_sequence()
                || record.sequence == 0
            {
                return Err(bad());
            }
            // The codec has refused a Missing with a stamp or logical, and a
            // RowValue without them or with another creator's Transaction
            // stamp; the rest of the outline table is judged here. A valid
            // Chunks or RowDeleted before is outside this path: the typed
            // Unsupported, Unchanged and unfenced (consumers F02 order,
            // MCB04-V14); corruption is never lowered to Unsupported.
            let value = match outline.before_kind {
                UndoBeforeKind::Missing => {
                    if outline.before_stamp.is_some()
                        || outline.before_logical
                        || outline.payload_form != UndoPayloadForm::None
                    {
                        return Err(bad());
                    }
                    false
                }
                UndoBeforeKind::RowValue if outline.payload_form == UndoPayloadForm::Chunks => {
                    return Err(unchanged(
                        RetentionPhase::Census,
                        RetentionCause::Unsupported,
                    ));
                }
                UndoBeforeKind::RowDeleted => {
                    return Err(unchanged(
                        RetentionPhase::Census,
                        RetentionCause::Unsupported,
                    ));
                }
                UndoBeforeKind::RowValue => {
                    if outline.before_stamp != Some(UndoStampKind::Committed)
                        || !outline.before_logical
                        || outline.payload_form != UndoPayloadForm::Inline
                        || !(5..=2048).contains(&outline.payload_bytes)
                    {
                        return Err(bad());
                    }
                    true
                }
                // The codec admits no other before kind for a Row target.
                UndoBeforeKind::IndexAbsent
                | UndoBeforeKind::IndexPresent
                | UndoBeforeKind::CatalogValue => return Err(bad()),
            };
            Ok(MissingRowEvidence { value })
        })();
        let evidence = merge(result, guard.release_checked())?;
        if evidence.value {
            self.value_capture = candidate.capture_sequence;
        }
        Ok(ReadStep::Ready(evidence))
    }
    /// The U RowValue before of a tombstone (dev3-row-delete.md#read) or of
    /// an Inline candidate whose U `missing_before` reported as a value
    /// before in this capture (dev3-row-update.md#read): the same
    /// LOCATION/LOAD order as `missing_before`, then the record through
    /// entry (a) with the operation's Catalog schema. The old canonical is
    /// put in the copy buffer's before range; B judges `q_old` and only then
    /// calls `copy_before`. A Releasing U is corruption (Fatal).
    #[inline(never)]
    pub(in crate::storage) fn value_before(
        &mut self,
        candidate: &RowCandidate<'b>,
        residency: &mut Plan4ResidencyCursor,
    ) -> Result<ReadStep<RowValueBefore<'b>>> {
        self.check_candidate(candidate)?;
        if !candidate.deleted && self.value_capture != candidate.capture_sequence {
            return Err(unchanged(
                RetentionPhase::Census,
                RetentionCause::WrongState,
            ));
        }
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
        self.read_value_before(candidate, reference, table, request)
    }
    #[inline(never)]
    fn read_value_before(
        &mut self,
        candidate: &RowCandidate<'b>,
        reference: crate::storage::undo_ref::UndoRef,
        table: ObjectHandle,
        request: TargetRequest,
    ) -> Result<ReadStep<RowValueBefore<'b>>> {
        use crate::storage::{
            row_record::RowSchemaInput,
            undo_record::{self, UndoRecordExpectation, UndoTargetKind, UndoTargetView},
        };
        let storage = self.backend.storage;
        let bad = || fatal(storage, RetentionPhase::Census, RetentionCause::Invariant);
        let (schema, copy) = self.copy.split_at_mut(VALUE_COPY);
        let record = catalog_record::decode_catalog_record(
            &schema[..self.schema_len],
            CatalogExpectation {
                namespace: storage.profile().spec().namespace,
                object: table.object,
            },
        )
        .map_err(|e| unchanged(RetentionPhase::Census, RetentionCause::Catalog(e)))?;
        let CatalogDefinitionView::Table(schema) = record.definition() else {
            return Err(unchanged(RetentionPhase::Census, RetentionCause::Invariant));
        };
        let guard = self
            .backend
            .read_catalog_undo(self.owner, &Plan4Target { request })?;
        let result = (|| {
            let page =
                crate::storage::page::decode_page(guard.bytes()?, request.identity, PageKind::Undo)
                    .map_err(|e| fatal(storage, RetentionPhase::Census, RetentionCause::Page(e)))?;
            let cell = page.cell(usize::from(reference.slot())).ok_or_else(bad)?;
            let expected = UndoRecordExpectation {
                namespace: storage.profile().spec().namespace,
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
            let record =
                undo_record::decode_undo_record(cell, expected, &RowSchemaInput::Catalog(schema))
                    .and_then(|view| view.live_before())
                    .map_err(|e| fatal(storage, RetentionPhase::Census, RetentionCause::Undo(e)))?;
            let (q_old, logical, value) =
                tombstone_before(&record, &candidate.version).ok_or_else(bad)?;
            copy[BEFORE_COPY - VALUE_COPY..][..value.len()].copy_from_slice(value);
            Ok(RowValueBefore {
                binding: std::marker::PhantomData,
                lease_identity: candidate.lease_identity,
                capture_sequence: candidate.capture_sequence,
                row: candidate.version.row,
                q_old,
                logical,
                canonical_len: value.len(),
            })
        })();
        merge(result, guard.release_checked()).map(ReadStep::Ready)
    }
    /// Copies the old value `value_before` put in the copy buffer; B has
    /// judged it visible.
    pub(in crate::storage) fn copy_before(
        &mut self,
        before: &RowValueBefore<'b>,
        output: &mut [u8],
    ) -> Result<RowCopyObservation> {
        if self.stopped
            || self.lease.as_ref().map(|l| l.row_identity()) != Some(before.lease_identity)
            || before.capture_sequence != self.capture_sequence
            || self.captured.is_none()
        {
            return Err(unchanged(
                RetentionPhase::Census,
                RetentionCause::WrongState,
            ));
        }
        self.statement.check_catalog_read()?;
        if output.len() < before.canonical_len {
            return Err(unchanged(RetentionPhase::Census, RetentionCause::Capacity));
        }
        output[..before.canonical_len]
            .copy_from_slice(&self.copy[BEFORE_COPY..BEFORE_COPY + before.canonical_len]);
        Ok(RowCopyObservation {
            version: RecordVersion {
                row: before.row,
                stamp: crate::storage::transaction_record::VersionStamp::Committed(before.q_old),
                logical: before.logical,
                undo: None,
            },
            len: before.canonical_len,
        })
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
        if candidate.deleted {
            return Err(unchanged(
                RetentionPhase::Census,
                RetentionCause::WrongState,
            ));
        }
        if output.len() < candidate.canonical_len {
            return Err(unchanged(RetentionPhase::Census, RetentionCause::Capacity));
        }
        output[..candidate.canonical_len]
            .copy_from_slice(&self.copy[VALUE_COPY..VALUE_COPY + candidate.canonical_len]);
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
            let (schema, copy) = self.copy.split_at_mut(VALUE_COPY);
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
            // capture validated an Inline row or a tombstone.
            let (len, deleted) = match record.payload() {
                crate::storage::row_record::RowPayload::Inline(value) => {
                    let len = value.as_bytes().len();
                    copy[..len].copy_from_slice(value.as_bytes());
                    (len, false)
                }
                _ => (0, true),
            };
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
                deleted,
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
impl<'s, 'c, W: WriterNamespace> ReadPort<'_, '_, 's, 'c, '_, '_, '_, '_, W> {
    /// S11 (MCB03-V01): the visibility registry's original
    /// `begin_retirement` of `undo` under `owner`'s version hold, with a
    /// test-built receipt and past the terminal read gate
    /// (`BoundVisibility::test_begin_undo_free`); an accepted intent is
    /// cancelled at once.
    pub(in crate::storage) fn row_test_begin_undo_free(
        &self,
        owner: &mut Owner<'c, 's>,
        undo: UndoRef,
        actor: crate::storage::transaction_record::TxnRef,
    ) -> std::result::Result<(), super::super::visibility::VisibilityError> {
        self.visibility
            .expect("actual visibility binding")
            .test_begin_undo_free(owner, undo, actor)
    }
}

#[cfg(test)]
impl<W: WriterNamespace> ReadPort<'_, '_, '_, '_, '_, '_, '_, '_, W> {
    /// Raw size observations, not an acceptance assertion or a replacement for
    /// the phase census. Loader and operation necessarily coexist at LOAD.
    pub(in crate::storage) fn row_test_load_inventory() -> [usize; 13] {
        use std::mem::size_of as n;
        let r = n::<&()>();
        // page: the key and the route's Result
        // (dev3-tree-smo.md#current-leaf-consumers).
        let page = 3 * r
            + n::<Result<()>>()
            + n::<Result<ReadStep<()>>>()
            + n::<RowId>()
            + n::<Result<ReadStep<LeafRoute>>>()
            + n::<Result<ReadStep<RowSearchPage<'_, '_, '_, '_, '_, '_, '_>>>>();
        let catalog = 3 * r
            + n::<Result<()>>()
            + n::<Result<[TargetRequest; 2]>>()
            + n::<std::ops::Range<usize>>()
            + n::<usize>()
            + n::<Result<RetentionResidencyStep>>();
        // load_leaf (dev3-tree-smo.md#current-leaf-consumers): the key, the
        // anchor request and its LOAD step, the step count, the walk's
        // Result, and the route page's LOAD step. The route page's request is
        // the LOAD's own argument.
        let leaf = 3 * r
            + n::<RowId>()
            + n::<TargetRequest>()
            + n::<Result<RetentionResidencyStep>>()
            + n::<u8>()
            + n::<Result<RouteNext>>()
            + n::<Result<RetentionResidencyStep>>();
        // route_resident: its arguments, the anchor root and its Result, the
        // key bytes, the route and level, the internal request and the step
        // Result, beside `anchor_root` or `route_internal`. Each of these is
        // its own values (the allocator request, the guard and its Result,
        // the membership and decode or route results) beside a two-target
        // read, the membership or the decode/`route_row_step`. No LOAD runs
        // under it.
        let read = |own: usize| {
            own + n::<TargetRequest>()
                + n::<ReadTargets<'_, '_, '_, '_, '_, '_, 2>>()
                + n::<Result<ReadTargets<'_, '_, '_, '_, '_, '_, 2>>>()
                + 2 * n::<Result<()>>()
                + 2 * n::<RetentionError>()
                + crate::storage::dirty_frame::target_set_call_bytes::<2>()
                    .max(
                        n::<crate::storage::allocation_page::AllocationPage<'_>>()
                            + 2 * n::<
                                std::result::Result<
                                    crate::storage::allocation_page::AllocationPage<'_>,
                                    crate::storage::allocation_page::AllocationError,
                                >,
                            >()
                            + 2 * n::<
                                std::result::Result<
                                    crate::storage::allocation_page::AllocationEntry,
                                    crate::storage::allocation_page::AllocationError,
                                >,
                            >()
                            + n::<usize>(),
                    )
                    .max(tree_page::search::route_step_source_bytes())
                    .max(
                        2 * n::<anchor::TreeAnchor>()
                            + 2 * n::<
                                std::result::Result<
                                    anchor::TreeAnchor,
                                    crate::storage::page::PageError,
                                >,
                            >()
                            + super::super::read::undo_decode_source_bytes(),
                    )
        };
        let internal = read(
            4 * r
                + n::<TargetRequest>()
                + n::<TreeExpectation>()
                + 2 * n::<std::result::Result<Option<TreeExpectation>, tree_page::TreeError>>()
                + 2 * n::<Result<Option<TreeExpectation>>>(),
        );
        let root = read(
            3 * r
                + n::<TargetRequest>()
                + 2 * n::<anchor::AnchorRoot>()
                + 2 * n::<Result<anchor::AnchorRoot>>(),
        );
        let route = 3 * r
            + n::<TargetRequest>()
            + n::<RowId>()
            + n::<anchor::AnchorRoot>()
            + 2 * n::<Result<anchor::AnchorRoot>>()
            + n::<[u8; 8]>()
            + n::<LeafRoute>()
            + n::<u8>()
            + n::<TargetRequest>()
            + 2 * n::<Result<Option<TreeExpectation>>>()
            + n::<Result<RouteNext>>()
            + internal.max(root);
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
            // value_before (dev3-row-delete.md#read): the same candidate/pins
            // and table/request observations with the before carrier.
            4 * r
                + n::<Result<()>>()
                + n::<Result<UndoRef>>()
                + 2 * n::<Option<&()>>()
                + 2 * n::<Option<UndoRef>>()
                + n::<Result<ObjectHandle>>()
                + n::<Result<TargetRequest>>()
                + n::<Result<RetentionResidencyStep>>()
                + n::<Result<ReadStep<RowValueBefore>>>(),
            route,
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

// S11 (MCB03-V08): one extra reference claim by the reading Owner itself,
// made once at its capture (beside `Point::RowReadCaptured`, while the
// operation holds its source pins), armed by the test on the reading
// thread. It calls the original `reserve_references` and records its
// answer; a lease it were granted is released at once.
#[cfg(test)]
thread_local! {
    static TEST_CAPTURE_CLAIM: std::cell::Cell<(
        bool,
        Option<std::result::Result<(), super::super::visibility::VisibilityError>>,
    )> = const { std::cell::Cell::new((false, None)) };
}

/// Arms the extra claim of the next Row read capture on this thread.
#[cfg(test)]
pub(in crate::storage) fn test_arm_capture_claim() {
    TEST_CAPTURE_CLAIM.with(|state| {
        assert_eq!(state.replace((true, None)), (false, None));
    });
}

/// The armed claim's answer (Ok if a lease was granted and released), once.
#[cfg(test)]
pub(in crate::storage) fn test_capture_claim()
-> Option<std::result::Result<(), super::super::visibility::VisibilityError>> {
    TEST_CAPTURE_CLAIM.with(|state| state.replace((false, None)).1)
}

#[cfg(test)]
fn test_capture_claim_probe<'a, 'c, 's>(
    visibility: Option<&super::super::visibility::BoundVisibility<'a, 'c, 's>>,
    owner: &Owner<'c, 's>,
) {
    TEST_CAPTURE_CLAIM.with(|state| {
        if !state.get().0 {
            return;
        }
        let answer = visibility
            .expect("actual visibility binding")
            .reserve_references(owner)
            .and_then(|lease| lease.release_checked());
        state.set((false, Some(answer)));
    });
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
            deleted: self.deleted,
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

/// The before of a tombstone's or an updated row's Live U
/// (dev3-row-delete.md#read, dev3-row-update.md#read): the U of the
/// version's own transaction (and Status generation) and mutation, whose
/// before is the committed Inline row it replaced (RowValue kind,
/// `Committed(q_old)`, its logical and value). Any other U (a Missing or
/// RowDeleted before, a `Transaction` before stamp, no value, another
/// creator or mutation, sequence 0), or a version not stamped `Transaction`
/// by that creator, is corruption: None. A Releasing U never reaches here
/// (`live_before` refuses it).
#[inline(always)]
fn tombstone_before<'r>(
    record: &crate::storage::undo_record::UndoRecord<'r>,
    version: &crate::storage::row_record::RecordVersion,
) -> Option<(
    foundation::identity::CommitSeq,
    crate::storage::transaction_record::LogicalRowVersion,
    &'r [u8],
)> {
    use crate::storage::{
        transaction_record::VersionStamp,
        undo_record::{UndoBeforeKind, UndoPayload},
    };
    let VersionStamp::Transaction(actor) = version.stamp else {
        return None;
    };
    if record.creator.id != version.logical.transaction()
        || actor.id() != record.creator.id
        || actor.generation() != record.creator.status_generation
        || record.mutation_sequence != version.logical.mutation_sequence()
        || record.sequence == 0
    {
        return None;
    }
    match (
        record.before_kind,
        record.before_stamp,
        record.before_logical,
        record.payload,
    ) {
        (
            UndoBeforeKind::RowValue,
            Some(VersionStamp::Committed(q_old)),
            Some(logical),
            UndoPayload::Inline(value),
        ) => Some((q_old, logical, value)),
        _ => None,
    }
}

#[cfg(test)]
#[path = "read/tombstone_tests.rs"]
mod tombstone_tests;

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
