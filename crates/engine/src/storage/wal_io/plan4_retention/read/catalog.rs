//! Typed physical catalog reads under the original whole-set pin. This does not
//! select transaction visibility or authorize catalog mutation/load/replay.
use super::*;
use crate::transaction::concurrency::PageConcurrency;
use crate::{
    api::Namespace,
    storage::{
        catalog_record::{self, CatalogExpectation, CatalogRecordView},
        dirty_frame::DirtyObservation,
        page::{self, PageHeader, PageView},
        tree_page::{EntryPayload, SearchPosition, TreeEntry, TreePageView},
    },
};

pub(in super::super) const CATALOG_READ_SOURCE_BYTES: usize = 16_384;

/// Holds allocator, anchor and leaf pins/latches together. Returned records
/// borrow this guard; releasing it ends every view. No visibility, existence of
/// a definition's own anchor, or durable catalog creation is certified here.
pub(crate) struct RetentionCatalogGuard<'a, 'b, 'c, 's, 'p, 'd, 'm, 'w, W: WriterNamespace> {
    read: ReadTargets<'a, 'b, 'c, 's, 'p, 'd, 3>,
    storage: &'s MutableStorageOwner<'m, 'w, W>,
    leaf: PageIdentity,
    core: &'c PageConcurrency<'s>,
}

/// Shared with the Row write preflight's census of the Catalog leaf copy.
pub(in super::super) fn leaf_view(
    bytes: &[u8],
    identity: PageIdentity,
    namespace: Namespace,
) -> std::result::Result<TreePageView<'_>, RetentionCause> {
    let view = tree_page::decode_tree_page(
        bytes,
        TreeExpectation {
            identity,
            kind: PageKind::CatalogLeaf,
            level: 0,
            arity: 0,
        },
    )
    .map_err(RetentionCause::Tree)?;
    // Validate every value, including cells outside a later lookup's search
    // path. A valid tree envelope alone does not certify typed catalog bytes.
    for index in 0..view.len() {
        let entry = view.entry(index).map_err(RetentionCause::Tree)?;
        record(entry, namespace)?;
    }
    Ok(view)
}

pub(in super::super) fn record(
    entry: TreeEntry<'_>,
    namespace: Namespace,
) -> std::result::Result<CatalogRecordView<'_>, RetentionCause> {
    let key: [u8; 8] = entry
        .key
        .try_into()
        .map_err(|_| RetentionCause::Invariant)?;
    let object = ObjectId::new(u64::from_be_bytes(key)).map_err(|_| RetentionCause::Invariant)?;
    let EntryPayload::Value(bytes) = entry.payload else {
        return Err(RetentionCause::Invariant);
    };
    catalog_record::decode_catalog_record(bytes, CatalogExpectation { namespace, object })
        .map_err(RetentionCause::Catalog)
}

fn lookup<'a>(
    view: &TreePageView<'a>,
    namespace: Namespace,
    object: ObjectId,
) -> std::result::Result<Option<CatalogRecordView<'a>>, RetentionCause> {
    let position = view
        .lower_bound(&object.get().to_be_bytes())
        .map_err(RetentionCause::Tree)?;
    if !position.equal {
        return Ok(None);
    }
    record(
        view.entry(position.index).map_err(RetentionCause::Tree)?,
        namespace,
    )
    .map(Some)
}

impl<'c, 's, W: WriterNamespace> RetentionCatalogGuard<'_, '_, 'c, 's, '_, '_, '_, '_, W> {
    pub(in super::super) fn source_origin(
        &self,
    ) -> (
        &'c PageConcurrency<'s>,
        &'s std::sync::atomic::AtomicU8,
        &'s crate::storage::session_fence::SessionFence,
    ) {
        (
            self.core,
            &self.storage.binding,
            self.storage.session_fence(),
        )
    }

    pub(in super::super) fn value(&self, object: ObjectId) -> Result<Option<&[u8]>> {
        let bytes = self
            .read
            .bytes(2)
            .map_err(|e| frame(RetentionPhase::Census, e))?;
        let view = leaf_view(bytes, self.leaf, self.storage.profile().spec().namespace)
            .map_err(|cause| fatal(self.storage, RetentionPhase::Census, cause))?;
        let position = view.lower_bound(&object.get().to_be_bytes()).map_err(|e| {
            fatal(
                self.storage,
                RetentionPhase::Census,
                RetentionCause::Tree(e),
            )
        })?;
        if !position.equal {
            return Ok(None);
        }
        let entry = view.entry(position.index).map_err(|e| {
            fatal(
                self.storage,
                RetentionPhase::Census,
                RetentionCause::Tree(e),
            )
        })?;
        let EntryPayload::Value(value) = entry.payload else {
            return Err(fatal(
                self.storage,
                RetentionPhase::Census,
                RetentionCause::Invariant,
            ));
        };
        Ok(Some(value))
    }

    /// Return the stored physical version, including retired/uncommitted cells.
    /// ObjectId already excludes zero; an absent object is a normal `None`.
    /// Stored corruption fences the original session and preserves its cause.
    pub(crate) fn lookup(&self, object: ObjectId) -> Result<Option<CatalogRecordView<'_>>> {
        let bytes = self
            .read
            .bytes(2)
            .map_err(|e| frame(RetentionPhase::Census, e))?;
        let namespace = self.storage.profile().spec().namespace;
        let view = leaf_view(bytes, self.leaf, namespace)
            .map_err(|cause| fatal(self.storage, RetentionPhase::Census, cause))?;
        lookup(&view, namespace, object)
            .map_err(|cause| fatal(self.storage, RetentionPhase::Census, cause))
    }

    pub(crate) fn release_checked(self) -> Result<()> {
        self.read
            .release_checked()
            .map_err(|e| frame(RetentionPhase::Census, e))
    }
}

impl<'b, 'arena, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    RetentionReader<'b, 'arena, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    /// Borrow the resident fixed catalog root through the original pool. Call
    /// `prepare_catalog` first; this never loads, allocates, stages WAL or writes.
    ///
    /// Semantic lookup captures references under this guard and copies the
    /// value before releasing these physical source latches.
    pub(crate) fn read_catalog<'a>(
        &self,
        owner: &'a mut Owner<'c, 's>,
    ) -> Result<RetentionCatalogGuard<'a, 'b, 'c, 's, 'p, 'd, 'm, 'w, W>> {
        health(self.storage, self.pool)?;
        if catalog_source_bytes::<W>() > CATALOG_READ_SOURCE_BYTES {
            return Err(unchanged(RetentionPhase::Census, RetentionCause::Capacity));
        }
        self.pool
            .check_retention_reader(owner)
            .map_err(|e| frame(RetentionPhase::Census, e))?;
        let allocator = self
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Census, e))?;
        let header = |index| {
            super::super::super::genesis::owned_header(&self.storage.core.plan, index).map_err(
                |_| {
                    fatal(
                        self.storage,
                        RetentionPhase::Census,
                        RetentionCause::Invariant,
                    )
                },
            )
        };
        let anchor = header(7)?;
        let leaf = header(8)?;
        let requests = [
            allocator,
            TargetRequest {
                identity: anchor.identity,
                kind: anchor.kind,
                level: 0,
            },
            TargetRequest {
                identity: leaf.identity,
                kind: leaf.kind,
                level: 0,
            },
        ];
        let read = self
            .pool
            .reserve_target_set(owner, &requests)
            .map_err(|e| frame(RetentionPhase::Census, e))?
            .try_read()
            .map_err(|e| frame(RetentionPhase::Census, e))?;
        let checked = (|| {
            let allocation = read
                .bytes(0)
                .map_err(|e| frame(RetentionPhase::Census, e))?;
            for (index, request) in requests.iter().enumerate().skip(1) {
                membership(self.storage, allocator, allocation, *request)?;
                let observed = read
                    .observe(index)
                    .map_err(|e| frame(RetentionPhase::Census, e))?;
                let bytes = read
                    .bytes(index)
                    .map_err(|e| frame(RetentionPhase::Census, e))?;
                let envelope =
                    page::decode_page(bytes, request.identity, request.kind).map_err(|e| {
                        fatal(
                            self.storage,
                            RetentionPhase::Census,
                            RetentionCause::Page(e),
                        )
                    })?;
                if envelope.header().lsn != observed.frame.page_lsn {
                    return Err(fatal(
                        self.storage,
                        RetentionPhase::Census,
                        RetentionCause::Invariant,
                    ));
                }
            }
            let bytes = read
                .bytes(1)
                .map_err(|e| frame(RetentionPhase::Census, e))?;
            let decoded = anchor::decode_anchor(bytes, anchor.identity).map_err(|e| {
                fatal(
                    self.storage,
                    RetentionPhase::Census,
                    RetentionCause::Page(e),
                )
            })?;
            let root = decoded.root();
            if root.tree != TreeKind::Catalog
                || root.page != leaf.identity.page
                || root.level != 0
                || root.revision != 1
            {
                return Err(fatal(
                    self.storage,
                    RetentionPhase::Census,
                    RetentionCause::Invariant,
                ));
            }
            let bytes = read
                .bytes(2)
                .map_err(|e| frame(RetentionPhase::Census, e))?;
            decoded.decode_root(bytes).map_err(|e| {
                fatal(
                    self.storage,
                    RetentionPhase::Census,
                    RetentionCause::Page(e),
                )
            })?;
            leaf_view(
                bytes,
                leaf.identity,
                self.storage.profile().spec().namespace,
            )
            .map_err(|cause| fatal(self.storage, RetentionPhase::Census, cause))?;
            Ok(())
        })();
        match checked {
            Ok(()) => Ok(RetentionCatalogGuard {
                read,
                storage: self.storage,
                leaf: leaf.identity,
                core: self.pool.bound_core(),
            }),
            Err(error) => merge(
                Err(error),
                read.release_checked()
                    .map_err(|e| frame(RetentionPhase::Census, e)),
            ),
        }
    }
}

/// Conservative source values, including return carriers and nested decoding.
/// No caller page bytes are copied; the existing pool supplies all three pins.
pub(in super::super) fn catalog_source_bytes<W: WriterNamespace>() -> usize {
    use std::mem::size_of;
    type Guard<'a, W> = RetentionCatalogGuard<'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    dirty_frame::target_set_call_bytes::<3>()
        + 2 * size_of::<Guard<'_, W>>()
        + size_of::<Result<Guard<'_, W>>>()
        + 2 * size_of::<TreePageView<'_>>()
        + 2 * size_of::<std::result::Result<TreePageView<'_>, RetentionCause>>()
        + 4 * size_of::<CatalogRecordView<'_>>()
        + 4 * size_of::<std::result::Result<CatalogRecordView<'_>, RetentionCause>>()
        + 2 * size_of::<Result<Option<CatalogRecordView<'_>>>>()
        + 2 * size_of::<anchor::TreeAnchor>()
        + 2 * size_of::<anchor::AnchorRoot>()
        + 4 * size_of::<PageHeader>()
        + 2 * size_of::<PageView<'_>>()
        + 2 * size_of::<TreeExpectation>()
        + 2 * size_of::<TreeEntry<'_>>()
        + 2 * size_of::<SearchPosition>()
        + 4 * size_of::<TargetRequest>()
        + 2 * size_of::<DirtyObservation>()
        + 4 * size_of::<Result<()>>()
        + 2048
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
