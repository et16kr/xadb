//! Private epoch bookkeeping fed only by the original checked Catalog adapters.
//! Decoded identities are observations; activation remains inside exclusive
//! source validation and changes remain inside the original writer borrow.
use super::super::{control, create_pair::PreparedCatalog, history, recovery};
use super::*;
use crate::storage::{
    allocation_page::{self, AllocationEntry},
    anchor::{self, AnchorRoot, TreeKind},
    catalog_record::{self, CatalogDefinitionView, CatalogExpectation},
    control::Profile,
    page::{PAGE_BYTES, PageIdentity, PageKind, PageRef},
    retention_page::catalog::{self as queue, JobState},
    semantic_catalog::Operation,
    transaction_record::VersionStamp,
    tree_page::{self, EntryPayload, TreeExpectation},
};
use foundation::identity::{ObjectHandle, ObjectId};

const TABLES: usize = 32;
const HEADER: usize = 64;
const ENTRY: usize = 48;
const RESERVED: u8 = 1;
const PUBLISHED: u8 = 2;
const _: () = assert!(HEADER + TABLES * ENTRY == EPOCH_BYTES);

fn row(bytes: &[u8; EPOCH_BYTES], slot: usize) -> &[u8] {
    &bytes[HEADER + slot * ENTRY..HEADER + (slot + 1) * ENTRY]
}
fn row_mut(bytes: &mut [u8; EPOCH_BYTES], slot: usize) -> &mut [u8] {
    &mut bytes[HEADER + slot * ENTRY..HEADER + (slot + 1) * ENTRY]
}
fn same(row: &[u8], object: ObjectHandle) -> bool {
    u64_at(row, 8) == object.object.get()
        && u64_at(row, 16) == object.generation.get()
        && u64_at(row, 24) == object.schema_epoch.get()
}
fn insert(bytes: &mut [u8; EPOCH_BYTES], slot: usize, object: ObjectHandle, state: u8, q: u64) {
    let count = u64_at(bytes, 8);
    let row = row_mut(bytes, slot);
    row.fill(0);
    row[0] = state;
    put_u64(row, 8, object.object.get());
    put_u64(row, 16, object.generation.get());
    put_u64(row, 24, object.schema_epoch.get());
    put_u64(row, 32, q);
    put_u64(bytes, 8, count + 1);
}
fn find(bytes: &[u8; EPOCH_BYTES], object: ObjectHandle) -> Option<usize> {
    (0..TABLES).find(|slot| row(bytes, *slot)[0] != 0 && same(row(bytes, *slot), object))
}

// A pending delta is carried only by the checked Reserve/Cancel/CLR adapter.
// It reserves capacity before effects under the unique writer borrow; no other
// writer can consume that slot before the complete installation below.
pub(in super::super) struct EpochChange {
    slot: usize,
    object: ObjectHandle,
    reserve: bool,
}
pub(in super::super) struct CommitEpochs {
    mask: u32,
    q: u64,
    digest: [u8; 32],
}

impl BoundVisibility<'_, '_, '_> {
    fn check_prepared(&self, prepared: &PreparedCatalog<'_, '_, '_>) -> VisibilityResult<()> {
        if !self.origin.same(&Origin {
            core: prepared.core(),
            binding: prepared.binding(),
            fence: prepared.fence(),
        }) {
            return Err(unchanged(VisibilityCause::BindingMismatch));
        }
        self.origin.health()
    }

    pub(in super::super) fn prepare_epoch_change(
        &self,
        prepared: &PreparedCatalog<'_, '_, '_>,
        profile: Profile,
    ) -> VisibilityResult<Option<EpochChange>> {
        self.check_prepared(prepared)?;
        if !matches!(
            prepared.operation(),
            Operation::Reserve | Operation::Cancel | Operation::Clr
        ) {
            return Ok(None);
        }
        let publication = self.publication()?;
        if !publication.epochs_active {
            return Ok(None); // Original recovery cleanup before final activation.
        }
        let reserve = prepared.operation() == Operation::Reserve;
        let ordinal = prepared
            .observation()
            .job_ordinal
            .ok_or_else(|| self.origin.fatal(VisibilityCause::Invariant))?;
        let image = if reserve {
            prepared
                .targets()
                .iter()
                .enumerate()
                .find(|(_, request)| request.kind == PageKind::RetentionJobs)
                .and_then(|(at, _)| prepared.after(at))
        } else {
            prepared.before_role(2)
        }
        .ok_or_else(|| self.origin.fatal(VisibilityCause::Invariant))?;
        let jobs = queue::decode_jobs_page(image, profile.spec().namespace, (ordinal / 15) as u8)
            .map_err(|_| self.origin.fatal(VisibilityCause::Invariant))?;
        let job = queue::decode_job(
            jobs.cell(ordinal % 15)
                .ok_or_else(|| self.origin.fatal(VisibilityCause::Invariant))?,
            profile.spec().namespace,
        )
        .map_err(|_| self.origin.fatal(VisibilityCause::Invariant))?;
        let expected = if prepared.operation() == Operation::Clr {
            JobState::RootRetained
        } else {
            JobState::RootReserved
        };
        if job.state() != expected || job.source() != prepared.observation().actor {
            return Err(self.origin.fatal(VisibilityCause::Invariant));
        }
        let object = job
            .object()
            .ok_or_else(|| self.origin.fatal(VisibilityCause::Invariant))?;
        let slot = if reserve {
            if find(publication.epochs, object).is_some() {
                return Err(self.origin.fatal(VisibilityCause::Invariant));
            }
            (0..TABLES)
                .find(|slot| row(publication.epochs, *slot)[0] == 0)
                .ok_or_else(|| unchanged(VisibilityCause::Capacity))?
        } else {
            let slot = find(publication.epochs, object)
                .ok_or_else(|| self.origin.fatal(VisibilityCause::Invariant))?;
            if row(publication.epochs, slot)[0] != RESERVED {
                return Err(self.origin.fatal(VisibilityCause::Invariant));
            }
            slot
        };
        Ok(Some(EpochChange {
            slot,
            object,
            reserve,
        }))
    }

    /// Called after the same adapter's whole installation and confirmed WAL.
    /// Only unobserved Reserved entries can be returned; Published is stable.
    pub(in super::super) fn apply_epoch_change(
        &self,
        change: Option<EpochChange>,
    ) -> VisibilityResult<()> {
        let Some(change) = change else {
            return Ok(());
        };
        let mut publication = self.publication()?;
        let bytes = &mut publication.epochs;
        let prior = row(bytes, change.slot);
        if change.reserve {
            if prior[0] != 0 {
                return Err(self.origin.fatal(VisibilityCause::Invariant));
            }
            insert(bytes, change.slot, change.object, RESERVED, 0);
        } else {
            if prior[0] != RESERVED || !same(prior, change.object) {
                return Err(self.origin.fatal(VisibilityCause::Invariant));
            }
            let count = u64_at(*bytes, 8)
                .checked_sub(1)
                .ok_or_else(|| self.origin.fatal(VisibilityCause::Invariant))?;
            row_mut(bytes, change.slot).fill(0);
            put_u64(*bytes, 8, count);
        }
        Ok(())
    }

    pub(in super::super) fn prepare_commit_epochs(
        &self,
        prepared: &PreparedCatalog<'_, '_, '_>,
        ledger: &control::RetentionLedger<'_, '_, '_>,
        profile: Profile,
        q: u64,
    ) -> VisibilityResult<CommitEpochs> {
        self.check_prepared(prepared)?;
        let publication = self.publication()?;
        if !publication.epochs_active
            || prepared.operation() != Operation::Commit
            || publication.q.checked_add(1) != Some(q)
        {
            return Err(self.origin.fatal(VisibilityCause::Invariant));
        }
        let actor = prepared
            .observation()
            .actor
            .ok_or_else(|| self.origin.fatal(VisibilityCause::Invariant))?;
        let mut mask = 0u32;
        for cell in ledger.jobs() {
            let job = queue::decode_job(cell, profile.spec().namespace)
                .map_err(|_| self.origin.fatal(VisibilityCause::Invariant))?;
            if job.source() != Some(actor) || !job.state().is_root() {
                continue;
            }
            // Compensated CREATE has no remaining logical Catalog change.
            if job.state() == JobState::RootReleasingCompensated {
                continue;
            }
            if job.state() != JobState::RootRetained {
                return Err(self.origin.fatal(VisibilityCause::Invariant));
            }
            let object = job
                .object()
                .ok_or_else(|| self.origin.fatal(VisibilityCause::Invariant))?;
            let slot = find(publication.epochs, object)
                .ok_or_else(|| self.origin.fatal(VisibilityCause::Invariant))?;
            if row(publication.epochs, slot)[0] != RESERVED || mask & (1 << slot) != 0 {
                return Err(self.origin.fatal(VisibilityCause::Invariant));
            }
            mask |= 1 << slot;
        }
        Ok(CommitEpochs {
            mask,
            q,
            digest: prepared.frame_digest(),
        })
    }

    pub(in super::super) fn publish_catalog_epochs(
        &self,
        installed: InstalledFrontier<'_, '_>,
        epochs: CommitEpochs,
    ) -> VisibilityResult<()> {
        if !self.origin.same(&installed.origin) {
            return Err(unchanged(VisibilityCause::BindingMismatch));
        }
        let mut publication = self.publication()?;
        if !publication.epochs_active
            || publication.q.checked_add(1) != Some(installed.q)
            || epochs.q != installed.q
            || epochs.digest != installed.digest
        {
            return Err(self.origin.fatal(VisibilityCause::Invariant));
        }
        for slot in 0..TABLES {
            if epochs.mask & (1 << slot) != 0 && row(publication.epochs, slot)[0] != RESERVED {
                return Err(self.origin.fatal(VisibilityCause::Invariant));
            }
        }
        for slot in 0..TABLES {
            if epochs.mask & (1 << slot) != 0 {
                let row = row_mut(publication.epochs, slot);
                row[0] = PUBLISHED;
                put_u64(row, 32, installed.q);
            }
        }
        if epochs.mask != 0 {
            put_u64(publication.epochs, 0, installed.q);
        }
        publication.q = installed.q;
        Ok(())
    }
}

/// Only original exclusive activation calls this after accepted replay. The
/// final Catalog must be stamped, with all original transaction Jobs drained.
/// Failure is fenced by that caller and never yields activation/READY.
pub(super) fn seed(
    profile: Profile,
    replay: &[u8],
    ledger: &control::RetentionLedger<'_, '_, '_>,
    bytes: &mut [u8; EPOCH_BYTES],
    q: u64,
) -> Result<(), ()> {
    let namespace = profile.spec().namespace;
    if profile.format() != 2 {
        return Err(());
    }
    for cell in ledger.jobs() {
        if queue::decode_job(cell, namespace).map_err(|_| ())?.state() != JobState::Free {
            return Err(());
        }
    }
    let base = recovery::allocator_identity(profile).map_err(|_| ())?;
    let allocation =
        allocation_page::decode_allocation_page(history::arena_page(replay, 0), base, namespace)
            .map_err(|_| ())?;
    let catalog_id = PageIdentity {
        page: PageRef {
            page_no: 7,
            ..base.page
        },
        owner: ObjectId::new(5).map_err(|_| ())?,
        ..base
    };
    let catalog =
        anchor::decode_anchor(history::arena_page(replay, 7), catalog_id).map_err(|_| ())?;
    let leaf_id = PageIdentity {
        page: PageRef {
            page_no: 8,
            ..base.page
        },
        ..catalog_id
    };
    if catalog.root()
        != (AnchorRoot {
            tree: TreeKind::Catalog,
            page: leaf_id.page,
            level: 0,
            revision: 1,
        })
    {
        return Err(());
    }
    let leaf = tree_page::decode_tree_page(
        history::arena_page(replay, 8),
        TreeExpectation {
            identity: leaf_id,
            kind: PageKind::CatalogLeaf,
            level: 0,
            arity: 0,
        },
    )
    .map_err(|_| ())?;
    bytes.fill(0);
    put_u64(bytes, 0, q);
    for at in 0..leaf.len() {
        if at >= TABLES {
            return Err(());
        }
        let entry = leaf.entry(at).map_err(|_| ())?;
        let object = ObjectId::new(u64::from_be_bytes(entry.key.try_into().map_err(|_| ())?))
            .map_err(|_| ())?;
        let EntryPayload::Value(value) = entry.payload else {
            return Err(());
        };
        let record =
            catalog_record::decode_catalog_record(value, CatalogExpectation { namespace, object })
                .map_err(|_| ())?;
        let header = record.header();
        if !matches!(record.definition(), CatalogDefinitionView::Table(_))
            || header.retired
            || header.undo.is_some()
            || !matches!(header.stamp, VersionStamp::Committed(c) if c.get() <= q)
        {
            return Err(());
        }
        let anchor_id = PageIdentity {
            page: header.anchor,
            owner: object,
            owner_generation: header.identity.generation,
            ..base
        };
        let checked_image = |page: PageRef, kind| -> Result<&[u8; PAGE_BYTES], ()> {
            if page.file != base.page.file
                || !(9..64).contains(&page.page_no)
                || allocation.entry(page.page_no as usize).map_err(|_| ())?
                    != (AllocationEntry::Allocated {
                        generation: page.incarnation,
                        object,
                        object_generation: header.identity.generation,
                        kind,
                    })
            {
                return Err(());
            }
            Ok(history::arena_page(replay, page.page_no as usize))
        };
        let anchor = anchor::decode_anchor(
            checked_image(header.anchor, PageKind::TreeAnchor)?,
            anchor_id,
        )
        .map_err(|_| ())?;
        let root = anchor.root();
        if root.tree != TreeKind::Row || root.level != 0 {
            return Err(());
        }
        tree_page::decode_tree_page(
            checked_image(root.page, PageKind::RowLeaf)?,
            TreeExpectation {
                identity: PageIdentity {
                    page: root.page,
                    ..anchor_id
                },
                kind: PageKind::RowLeaf,
                level: 0,
                arity: 0,
            },
        )
        .map_err(|_| ())?;
        insert(bytes, at, header.identity, PUBLISHED, q);
    }
    Ok(())
}

pub(in super::super) fn change_carriers() -> usize {
    std::mem::size_of::<Option<EpochChange>>()
        + std::mem::size_of::<VisibilityResult<Option<EpochChange>>>()
}
pub(in super::super) fn commit_carriers() -> usize {
    std::mem::size_of::<CommitEpochs>() + std::mem::size_of::<VisibilityResult<CommitEpochs>>()
}
/// The prepare/apply/publish helper's live scalar packet and decoder closure.
/// A Job page decode ends before the Reserved-slot write. Publication itself
/// calls no codec; its complete guard/receipt/check packet is charged here too.
pub(in super::super) fn transition_source_bytes() -> usize {
    use std::mem::size_of;
    let outer = 2 * size_of::<VisibilityResult<MutexGuard<'_, Publication<'_>>>>()
        + size_of::<MutexGuard<'_, Publication<'_>>>()
        + 2 * size_of::<Origin<'_, '_>>()
        + 2 * size_of::<ObjectHandle>()
        + 2 * size_of::<Option<usize>>()
        + 2 * size_of::<TxnRef>()
        + 2 * size_of::<Profile>()
        + 2 * size_of::<EpochChange>()
        + 2 * size_of::<CommitEpochs>()
        + size_of::<InstalledFrontier<'_, '_>>()
        + 4 * size_of::<VisibilityError>()
        + 8 * size_of::<VisibilityResult<()>>()
        + 16 * size_of::<&[u8]>()
        + 16 * size_of::<u64>();
    let decode = 2 * size_of::<crate::storage::page::PageView<'_>>()
        + 2 * size_of::<queue::JobView<'_>>()
        + 2 * size_of::<
            Result<
                crate::storage::page::PageView<'_>,
                crate::storage::retention_page::RetentionError,
            >,
        >()
        + 2 * size_of::<Result<queue::JobView<'_>, crate::storage::retention_page::RetentionError>>(
        )
        + queue::source_bytes();
    outer + decode
}

/// Borrowed decoder carriers and their existing codec closure; backing stays
/// in the original loan. This is a source census, not a compiler-stack estimate.
pub(super) fn source_bytes() -> usize {
    use std::mem::size_of;
    4 * size_of::<queue::JobView<'_>>()
        + 2 * size_of::<crate::storage::page::PageView<'_>>()
        + 2 * size_of::<catalog_record::CatalogRecordView<'_>>()
        + 2 * size_of::<tree_page::TreePageView<'_>>()
        + 2 * size_of::<allocation_page::AllocationPage<'_>>()
        + 4 * size_of::<anchor::TreeAnchor>()
        + 4 * size_of::<PageIdentity>()
        + 4 * size_of::<TreeExpectation>()
        + 32 * size_of::<u64>()
        + 32 * size_of::<&[u8]>()
        + crate::storage::semantic_catalog::source_census()
            .catalog_image
            .max(queue::source_bytes())
}

#[cfg(test)]
impl BoundVisibility<'_, '_, '_> {
    pub(in super::super) fn test_epochs(&self) -> (bool, u64, Vec<(u8, u64, u64)>) {
        let publication = self.published.lock().unwrap_or_else(|e| e.into_inner());
        (
            publication.epochs_active,
            u64_at(publication.epochs, 0),
            (0..TABLES)
                .filter_map(|slot| {
                    let row = row(publication.epochs, slot);
                    (row[0] != 0).then(|| (row[0], u64_at(row, 8), u64_at(row, 32)))
                })
                .collect(),
        )
    }
}
