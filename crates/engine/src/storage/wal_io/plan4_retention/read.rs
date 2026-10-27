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
#[path = "read/operation.rs"]
mod operation;
#[path = "read/resident.rs"]
mod resident;
#[path = "read/scope.rs"]
mod scope;
#[path = "read/statement.rs"]
mod statement;
pub(crate) use crate::storage::catalog::read::CatalogLookupStep;
#[allow(
    unused_imports,
    reason = "typed guard name for the following catalog consumer"
)]
pub(crate) use catalog::RetentionCatalogGuard;
pub(super) use catalog::{CATALOG_READ_SOURCE_BYTES, catalog_source_bytes};
pub(super) use lookup::resident_source_bytes as resident_lookup_source_bytes;
pub(super) use lookup::source_bytes as lookup_source_bytes;
pub(crate) use operation::{
    CatalogCandidate, CatalogReadOperation, MissingCatalogEvidence, ReadStep,
};
pub(crate) use scope::{CatalogCallError, CatalogCallScope};
pub(crate) use statement::CatalogStatementCall;

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
/// The expected shape of a one-slot Undo root page under a read guard.
#[derive(Clone, Copy, PartialEq, Eq)]
enum UndoShape {
    /// The initial Free page of the original Undo container.
    InitialFree,
    /// A CREATE root page whose single record the caller decodes with its own
    /// exact expectation; Live/Releasing records are valid here.
    CatalogRoot,
}

impl<'b, 'arena, 's, 'c, 'p, 'd, W: WriterNamespace>
    RetentionReader<'b, 'arena, 's, 'c, 'p, 'd, '_, '_, W>
{
    pub(crate) fn read<'a>(
        &self,
        owner: &'a mut Owner<'c, 's>,
        target: &Plan4Target,
    ) -> Result<RetentionReadGuard<'a, 'b, 'c, 's, 'p, 'd>> {
        self.read_shaped(owner, target, UndoShape::InitialFree)
    }
    /// Catalog lookup reads the referenced Undo root of a live CREATE.
    fn read_catalog_undo<'a>(
        &self,
        owner: &'a mut Owner<'c, 's>,
        target: &Plan4Target,
    ) -> Result<RetentionReadGuard<'a, 'b, 'c, 's, 'p, 'd>> {
        if target.request.kind != PageKind::Undo {
            return Err(unchanged(
                RetentionPhase::Census,
                RetentionCause::Membership,
            ));
        }
        self.read_shaped(owner, target, UndoShape::CatalogRoot)
    }
    fn read_shaped<'a>(
        &self,
        owner: &'a mut Owner<'c, 's>,
        target: &Plan4Target,
        shape: UndoShape,
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
                    // The bound Profile selects the grammar. A decoder failure
                    // must not retry another version and admit mixed pages.
                    if self.storage.profile().format() == 2 {
                        retention_page::catalog::decode_root_page(
                            bytes,
                            self.storage.profile().spec().namespace,
                        )
                        .map(|_| ())
                    } else {
                        retention_page::decode_root_page(
                            bytes,
                            self.storage.profile().spec().namespace,
                        )
                        .map(|_| ())
                    }
                    .map_err(|e| {
                        fatal(
                            self.storage,
                            RetentionPhase::Census,
                            RetentionCause::Retention(e),
                        )
                    })?;
                }
                PageKind::RetentionJobs => {
                    if self.storage.profile().format() == 2 {
                        retention_page::catalog::decode_jobs_page(
                            bytes,
                            self.storage.profile().spec().namespace,
                            (target.request.identity.page.page_no - 2) as u8,
                        )
                    } else {
                        retention_page::decode_jobs_page(
                            bytes,
                            self.storage.profile().spec().namespace,
                            (target.request.identity.page.page_no - 2) as u8,
                        )
                    }
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
                    let lsn = match shape {
                        UndoShape::InitialFree => {
                            crate::storage::undo_record::initial_undo_page_lsn(
                                bytes,
                                target.request.identity,
                            )
                            .map_err(|e| {
                                fatal(
                                    self.storage,
                                    RetentionPhase::Census,
                                    RetentionCause::Undo(e),
                                )
                            })?
                        }
                        UndoShape::CatalogRoot => {
                            let page = crate::storage::page::decode_page(
                                bytes,
                                target.request.identity,
                                PageKind::Undo,
                            )
                            .map_err(|e| {
                                fatal(
                                    self.storage,
                                    RetentionPhase::Census,
                                    RetentionCause::Page(e),
                                )
                            })?;
                            if page.len() != 1 {
                                return Err(fatal(
                                    self.storage,
                                    RetentionPhase::Census,
                                    RetentionCause::Invariant,
                                ));
                            }
                            page.header().lsn
                        }
                    };
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

/// Source values for the two-target reader. Undo and Catalog queue decoders
/// borrow their cells; charge the larger branch, including v2 Root/Jobs views.
pub(super) fn undo_read_source_bytes() -> usize {
    use std::mem::size_of;
    type Guard<'a> = RetentionReadGuard<'a, 'a, 'a, 'a, 'a, 'a>;
    dirty_frame::target_set_call_bytes::<2>()
        + 2 * size_of::<UndoShape>()
        + 2 * size_of::<Guard<'_>>()
        + 2 * size_of::<Result<Guard<'_>>>()
        + undo_decode_source_bytes().max(retention_page::catalog::source_bytes())
        + 2 * size_of::<allocation_page::AllocationPage<'_>>()
        + 2 * size_of::<dirty_frame::DirtyObservation>()
        + 4 * size_of::<TargetRequest>()
        + 4 * size_of::<Result<()>>()
        + 1024
}

/// Resident lookup invokes read_shaped only for allocator-issued Status and
/// exact CatalogRoot Undo requests. Root/Jobs/InitialFree are unreachable from
/// that caller, so their unrelated queue decoder is excluded here.
pub(super) fn catalog_reference_read_source_bytes() -> usize {
    use std::mem::size_of;
    type Guard<'a> = RetentionReadGuard<'a, 'a, 'a, 'a, 'a, 'a>;
    dirty_frame::target_set_call_bytes::<2>()
        + 2 * size_of::<UndoShape>()
        + 2 * size_of::<Guard<'_>>()
        + 2 * size_of::<Result<Guard<'_>>>()
        + crate::storage::semantic_catalog::page_read_codec_bytes()
            .max(crate::storage::semantic_catalog::status_read_codec_bytes())
        + 2 * size_of::<allocation_page::AllocationPage<'_>>()
        + 2 * size_of::<AllocationEntry>()
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

#[cfg(test)]
pub(super) fn catalog_call_source_observations<W: WriterNamespace>() -> [usize; 5] {
    [
        scope::held_source_bytes::<W>(),
        scope::entry_source_bytes::<W>(),
        scope::consumer_source_bytes::<W>(),
        lookup_source_bytes::<W>(),
        resident_lookup_source_bytes::<W>(),
    ]
}
