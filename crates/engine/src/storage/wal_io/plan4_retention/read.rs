//! Read actors borrow the same original registrations and checked history loan.
//! The mutable writer ledger and flush workspace remain with their own actors.
use super::*;
use crate::storage::{
    allocation_page::{self, AllocationEntry},
    anchor::{self, TreeKind},
    dirty_frame::ReadTargets,
    page::PageKind,
    tree_page::{self, TreeExpectation},
};
use foundation::identity::ObjectId;

#[path = "read/catalog.rs"]
mod catalog;
#[path = "read/lookup.rs"]
mod lookup;
pub(crate) use lookup::CatalogLookupStep;
pub(super) use lookup::source_bytes as lookup_source_bytes;
#[allow(
    unused_imports,
    reason = "typed guard name for the following catalog consumer"
)]
pub(crate) use catalog::RetentionCatalogGuard;
pub(super) use catalog::{CATALOG_READ_SOURCE_BYTES, catalog_source_bytes};

pub(crate) struct RetentionReader<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace> {
    pub(super) storage: &'s MutableStorageOwner<'m, 'w, W>,
    pub(super) pool: &'b SharedPool<'c, 's, 'p, 'd>,
    pub(super) history: &'b history::runtime::RuntimeHistory<'a, 'c, 's>,
    pub(super) visibility: Option<&'b visibility::BoundVisibility<'a, 'c, 's>>,
}
pub(crate) struct RetentionReadGuard<'a, 'b, 'c, 's, 'p, 'd> {
    read: ReadTargets<'a, 'b, 'c, 's, 'p, 'd, 2>,
}
impl RetentionReadGuard<'_, '_, '_, '_, '_, '_> {
    pub(crate) fn bytes(&self) -> Result<&[u8; PAGE_BYTES]> {
        self.read
            .bytes(1)
            .map_err(|e| frame(RetentionPhase::Census, e))
    }
    pub(crate) fn release_checked(self) -> Result<()> {
        self.read
            .release_checked()
            .map_err(|e| frame(RetentionPhase::Census, e))
    }
}
fn membership<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    allocator: TargetRequest,
    allocation_bytes: &[u8; PAGE_BYTES],
    request: TargetRequest,
) -> Result<()> {
    let index = usize::try_from(request.identity.page.page_no)
        .map_err(|_| unchanged(RetentionPhase::Census, RetentionCause::Membership))?;
    if !(1..64).contains(&index)
        || request.identity.database != allocator.identity.database
        || request.identity.page.file != allocator.identity.page.file
        || request.level != 0
    {
        return Err(unchanged(
            RetentionPhase::Census,
            RetentionCause::Membership,
        ));
    }
    let allocation = allocation_page::decode_allocation_page(
        allocation_bytes,
        allocator.identity,
        storage.profile().spec().namespace,
    )
    .map_err(|e| {
        fatal(
            storage,
            RetentionPhase::Census,
            RetentionCause::Allocation(e),
        )
    })?;
    if allocation.entry(index).map_err(|e| {
        fatal(
            storage,
            RetentionPhase::Census,
            RetentionCause::Allocation(e),
        )
    })? != (AllocationEntry::Allocated {
        generation: request.identity.page.incarnation,
        object: request.identity.owner,
        object_generation: request.identity.owner_generation,
        kind: request.kind,
    }) {
        return Err(unchanged(
            RetentionPhase::Census,
            RetentionCause::Membership,
        ));
    }
    Ok(())
}
impl<'b, 'arena, 's, 'c, 'p, 'd, W: WriterNamespace>
    RetentionReader<'b, 'arena, 's, 'c, 'p, 'd, '_, '_, W>
{
    pub(crate) fn read<'a>(
        &'b self,
        owner: &'a mut Owner<'c, 's>,
        target: &Plan4Target,
    ) -> Result<RetentionReadGuard<'a, 'b, 'c, 's, 'p, 'd>> {
        health(self.storage, self.pool)?;
        self.pool
            .check_retention_reader(owner)
            .map_err(|e| frame(RetentionPhase::Census, e))?;
        let allocator = self
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Census, e))?;
        let read = self
            .pool
            .reserve_target_set(owner, &[allocator, target.request])
            .map_err(|e| frame(RetentionPhase::Census, e))?
            .try_read()
            .map_err(|e| frame(RetentionPhase::Census, e))?;
        let checked = (|| {
            membership(
                self.storage,
                allocator,
                read.bytes(0)
                    .map_err(|e| frame(RetentionPhase::Census, e))?,
                target.request,
            )?;
            let bytes = read
                .bytes(1)
                .map_err(|e| frame(RetentionPhase::Census, e))?;
            match target.request.kind {
                PageKind::RetentionRoot => {
                    retention_page::decode_root_page(
                        bytes,
                        self.storage.profile().spec().namespace,
                    )
                    .map_err(|e| {
                        fatal(
                            self.storage,
                            RetentionPhase::Census,
                            RetentionCause::Retention(e),
                        )
                    })?;
                }
                PageKind::RetentionJobs => {
                    retention_page::decode_jobs_page(
                        bytes,
                        self.storage.profile().spec().namespace,
                        (target.request.identity.page.page_no - 2) as u8,
                    )
                    .map_err(|e| {
                        fatal(
                            self.storage,
                            RetentionPhase::Census,
                            RetentionCause::Retention(e),
                        )
                    })?;
                }
                PageKind::TransactionStatus => {
                    let home = crate::storage::status_home::decode_status_home_page(
                        bytes,
                        target.request.identity,
                        self.storage.profile().spec().namespace,
                    )
                    .map_err(|e| {
                        fatal(
                            self.storage,
                            RetentionPhase::Census,
                            RetentionCause::Page(e),
                        )
                    })?;
                    if home.len() != 1 {
                        return Err(fatal(
                            self.storage,
                            RetentionPhase::Census,
                            RetentionCause::Invariant,
                        ));
                    }
                }
                PageKind::Undo => {
                    let lsn = crate::storage::undo_record::initial_undo_page_lsn(
                        bytes,
                        target.request.identity,
                    )
                    .map_err(|e| {
                        fatal(
                            self.storage,
                            RetentionPhase::Census,
                            RetentionCause::Undo(e),
                        )
                    })?;
                    let observed = read
                        .observe(1)
                        .map_err(|e| frame(RetentionPhase::Census, e))?;
                    if lsn == PageLsn(0) || observed.frame.page_lsn != lsn {
                        return Err(fatal(
                            self.storage,
                            RetentionPhase::Census,
                            RetentionCause::Invariant,
                        ));
                    }
                }
                _ => {
                    return Err(unchanged(
                        RetentionPhase::Census,
                        RetentionCause::Membership,
                    ));
                }
            }
            Ok(())
        })();
        match checked {
            Ok(()) => Ok(RetentionReadGuard { read }),
            Err(error) => merge(
                Err(error),
                read.release_checked()
                    .map_err(|e| frame(RetentionPhase::Census, e)),
            ),
        }
    }
    pub(crate) fn lookup_catalog_empty(
        &self,
        owner: &mut Owner<'c, 's>,
        object: ObjectId,
    ) -> Result<super::super::Plan4CatalogLookup> {
        health(self.storage, self.pool)?;
        self.pool
            .check_retention_reader(owner)
            .map_err(|e| frame(RetentionPhase::Census, e))?;
        let allocator = self
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Census, e))?;
        let anchor =
            super::super::genesis::owned_header(&self.storage.core.plan, 7).map_err(|_| {
                fatal(
                    self.storage,
                    RetentionPhase::Census,
                    RetentionCause::Invariant,
                )
            })?;
        let leaf =
            super::super::genesis::owned_header(&self.storage.core.plan, 8).map_err(|_| {
                fatal(
                    self.storage,
                    RetentionPhase::Census,
                    RetentionCause::Invariant,
                )
            })?;
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
            for (at, request) in requests.iter().enumerate().skip(1) {
                membership(self.storage, allocator, allocation, *request)?;
                let observed = read
                    .observe(at)
                    .map_err(|e| frame(RetentionPhase::Census, e))?;
                if observed.frame.page_lsn != PageLsn(0)
                    || observed.frame.modification_counter != 0
                    || observed.entry.is_some()
                {
                    return Err(fatal(
                        self.storage,
                        RetentionPhase::Census,
                        RetentionCause::Invariant,
                    ));
                }
            }
            let anchor = anchor::decode_anchor(
                read.bytes(1)
                    .map_err(|e| frame(RetentionPhase::Census, e))?,
                anchor.identity,
            )
            .map_err(|e| {
                fatal(
                    self.storage,
                    RetentionPhase::Census,
                    RetentionCause::Page(e),
                )
            })?;
            let root = anchor.root();
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
            anchor.decode_root(bytes).map_err(|e| {
                fatal(
                    self.storage,
                    RetentionPhase::Census,
                    RetentionCause::Page(e),
                )
            })?;
            let leaf = tree_page::decode_tree_page(
                bytes,
                TreeExpectation {
                    identity: leaf.identity,
                    kind: PageKind::CatalogLeaf,
                    level: 0,
                    arity: 0,
                },
            )
            .map_err(|e| {
                fatal(
                    self.storage,
                    RetentionPhase::Census,
                    RetentionCause::Tree(e),
                )
            })?;
            let position = leaf.lower_bound(&object.get().to_be_bytes()).map_err(|e| {
                fatal(
                    self.storage,
                    RetentionPhase::Census,
                    RetentionCause::Tree(e),
                )
            })?;
            if leaf.len() != 0 || position.index != 0 || position.equal {
                return Err(fatal(
                    self.storage,
                    RetentionPhase::Census,
                    RetentionCause::Invariant,
                ));
            }
            Ok(super::super::Plan4CatalogLookup::NotFound)
        })();
        merge(
            checked,
            read.release_checked()
                .map_err(|e| frame(RetentionPhase::Census, e)),
        )
    }
}

/// Source values for the new two-target Free Undo reader. The decoder borrows
/// its cell; no page or 432-byte Free cell is copied during validation.
pub(super) fn undo_read_source_bytes() -> usize {
    use std::mem::size_of;
    type Guard<'a> = RetentionReadGuard<'a, 'a, 'a, 'a, 'a, 'a>;
    dirty_frame::target_set_call_bytes::<2>()
        + 2 * size_of::<Guard<'_>>()
        + 2 * size_of::<Result<Guard<'_>>>()
        + undo_decode_source_bytes()
        + 2 * size_of::<allocation_page::AllocationPage<'_>>()
        + 2 * size_of::<dirty_frame::DirtyObservation>()
        + 4 * size_of::<TargetRequest>()
        + 4 * size_of::<Result<()>>()
        + 1024
}

/// The canonical initial-Undo decoder uses a borrowed PageView and a Free
/// generation observation. Shared by reader/history/recovered census phases.
pub(super) fn undo_decode_source_bytes() -> usize {
    use crate::storage::{
        page::{PageError, PageHeader, PageView},
        undo_record::UndoError,
    };
    use std::mem::size_of;
    2 * size_of::<PageView<'_>>()
        + 2 * size_of::<std::result::Result<PageView<'_>, PageError>>()
        + 2 * size_of::<PageHeader>()
        + 2 * size_of::<std::result::Result<PageLsn, UndoError>>()
        + 2 * size_of::<std::result::Result<(), UndoError>>()
        + 2 * size_of::<std::ops::Range<usize>>()
        + 4 * size_of::<PageLsn>()
        + 4 * size_of::<foundation::identity::Generation>()
        + 8 * size_of::<&[u8]>()
}
