//! Actual whole-set consumers. No page guard survives a following load/window.
use super::*;
use crate::storage::{
    anchor::{self, TreeKind},
    dirty_frame::ReadTargets,
    page,
    tree_page::{self, TreeExpectation},
};
use xadb_foundation::codec::ContentHasher;

fn finish<T>(
    result: Result<T>,
    cleanup: dirty_frame::Result<()>,
    phase: Plan4PagePhase,
) -> Result<T> {
    match (result, cleanup) {
        (Err(error), _) if error.effect != Effect::Unchanged => Err(error),
        (_, Err(error)) => Err(frame(phase, error)),
        (result, Ok(())) => result,
    }
}

fn locate<const N: usize, W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    read: &ReadTargets<'_, '_, '_, '_, '_, '_, N>,
    request: TargetRequest,
    phase: Plan4PagePhase,
) -> Result<usize> {
    for index in 0..N {
        if read
            .target_request(index)
            .map_err(|error| frame(phase, error))?
            == request
        {
            return Ok(index);
        }
    }
    Err(fatal(storage, phase, Plan4PageCause::Invariant))
}

fn validate_set<const N: usize, W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    read: &ReadTargets<'_, '_, '_, '_, '_, '_, N>,
    canonical: &mut [u8; PAGE_BYTES],
    phase: Plan4PagePhase,
) -> Result<()> {
    for at in 0..N {
        let request = read
            .target_request(at)
            .map_err(|error| frame(phase, error))?;
        let observed = read.observe(at).map_err(|error| frame(phase, error))?;
        if observed.frame.page_lsn != PageLsn(0)
            || observed.frame.modification_counter != 0
            || observed.frame.rec_lsn.is_some()
            || observed.entry.is_some()
        {
            return Err(fatal(storage, phase, Plan4PageCause::Invariant));
        }
        let bytes = read.bytes(at).map_err(|error| frame(phase, error))?;
        let checked = if request.kind == PageKind::Allocation {
            genesis::verify_owned_allocator(&storage.core.plan, bytes)
        } else {
            let index = usize::try_from(request.identity.page.page_no)
                .map_err(|_| fatal(storage, phase, Plan4PageCause::Invariant))?;
            genesis::verify_owned_payload(&storage.core.plan, index, bytes, canonical)
        };
        checked.map_err(|cause| fatal(storage, phase, Plan4PageCause::Genesis(cause)))?;
    }
    Ok(())
}

impl<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> Plan4Pages<'s, 'c, 'p, 'd, 'o, 'm, 'w, W> {
    pub(crate) fn lookup_catalog_empty(
        &mut self,
        owner: &mut Owner<'c, 's>,
        object: ObjectId,
        cursor: &mut Plan4ResidencyCursor,
    ) -> Result<Plan4CatalogLookup> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        let p = Plan4PagePhase::Read;
        self.ready(p)?;
        // check_binding also requires Idle, before preparation can change RAM.
        self.pool
            .check_plan4_reader(owner)
            .map_err(|error| frame(p, error))?;
        let requests = [
            self.allocator_request(p)?,
            self.target(Plan4Page::CatalogAnchor)?.request,
            self.target(Plan4Page::CatalogLeaf)?.request,
        ];
        let mut next = *cursor;
        self.prepare_set(&requests, &mut next)?;
        let read = self
            .pool
            .reserve_target_set(owner, &requests)
            .and_then(|pinned| pinned.try_read())
            .map_err(|error| frame(p, error))?;
        let canonical = (&mut self.operation[4 * PAGE_BYTES..5 * PAGE_BYTES])
            .try_into()
            .expect("canonical read loan");
        let result = (|| {
            validate_set(storage, &read, canonical, p)?;
            let anchor_index = locate(storage, &read, requests[1], p)?;
            let leaf_index = locate(storage, &read, requests[2], p)?;
            let anchor_bytes = read.bytes(anchor_index).map_err(|error| frame(p, error))?;
            let leaf_bytes = read.bytes(leaf_index).map_err(|error| frame(p, error))?;
            let anchor = anchor::decode_anchor(anchor_bytes, requests[1].identity)
                .map_err(|error| fatal(storage, p, Plan4PageCause::Page(error)))?;
            let root = anchor.root();
            if root.tree != TreeKind::Catalog
                || root.page != requests[2].identity.page
                || root.level != 0
                || root.revision != 1
            {
                return Err(fatal(storage, p, Plan4PageCause::Invariant));
            }
            anchor
                .decode_root(leaf_bytes)
                .map_err(|error| fatal(storage, p, Plan4PageCause::Page(error)))?;
            let leaf = tree_page::decode_tree_page(
                leaf_bytes,
                TreeExpectation {
                    identity: requests[2].identity,
                    kind: PageKind::CatalogLeaf,
                    level: 0,
                    arity: 0,
                },
            )
            .map_err(|error| fatal(storage, p, Plan4PageCause::Tree(error)))?;
            let position = leaf
                .lower_bound(&object.get().to_be_bytes())
                .map_err(|error| fatal(storage, p, Plan4PageCause::Tree(error)))?;
            if leaf.len() != 0 || position.index != 0 || position.equal {
                return Err(fatal(storage, p, Plan4PageCause::Invariant));
            }
            Ok(Plan4CatalogLookup::NotFound)
        })();
        let result = finish(result, read.release_checked(), p)?;
        *cursor = next;
        Ok(result)
    }

    pub(crate) fn inspect_genesis_retention(
        &mut self,
        owner: &mut Owner<'c, 's>,
        cursor: &mut Plan4ResidencyCursor,
    ) -> Result<Plan4GenesisCensus> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        let p = Plan4PagePhase::Census;
        self.ready(p)?;
        self.pool
            .check_plan4_reader(owner)
            .map_err(|error| frame(p, error))?;
        let allocator = self.allocator_request(p)?;
        let root = self.target(Plan4Page::Root)?.request;
        let mut next = *cursor;
        let mut root_cell = [0u8; 512];
        let mut seen_root = false;
        let mut count = 0u16;
        let mut hash = ContentHasher::new();
        // Each call releases its complete read set before the next call loads.
        for first in [0u8, 2] {
            let requests = [
                allocator,
                root,
                self.target(Plan4Page::Jobs { index: first })?.request,
                self.target(Plan4Page::Jobs { index: first + 1 })?.request,
            ];
            self.queue_window(
                owner,
                &requests,
                &mut next,
                &mut root_cell,
                &mut seen_root,
                &mut hash,
                &mut count,
            )?;
        }
        let requests = [
            allocator,
            root,
            self.target(Plan4Page::Jobs { index: 4 })?.request,
        ];
        self.queue_window(
            owner,
            &requests,
            &mut next,
            &mut root_cell,
            &mut seen_root,
            &mut hash,
            &mut count,
        )?;
        let digest = hash.digest();
        let revision = u64::from_le_bytes(root_cell[64..72].try_into().expect("root revision"));
        let mask = u64::from_le_bytes(root_cell[96..104].try_into().expect("root Free mask"));
        if !seen_root
            || count != 64
            || revision != 1
            || mask != u64::MAX
            || root_cell[128..160] != digest
        {
            return Err(fatal(storage, p, Plan4PageCause::Invariant));
        }
        *cursor = next;
        Ok(Plan4GenesisCensus {
            revision,
            free_jobs: count,
            job_digest: digest,
        })
    }

    #[allow(clippy::too_many_arguments)] // Small phase state; no retained page/guard array.
    fn queue_window<const N: usize>(
        &mut self,
        owner: &mut Owner<'c, 's>,
        requests: &[TargetRequest; N],
        cursor: &mut Plan4ResidencyCursor,
        root_cell: &mut [u8; 512],
        seen_root: &mut bool,
        hash: &mut ContentHasher,
        count: &mut u16,
    ) -> Result<()> {
        const {
            assert!(N == 3 || N == 4);
        }
        let storage = self.storage;
        let p = Plan4PagePhase::Census;
        self.pool
            .check_plan4_reader(owner)
            .map_err(|error| frame(p, error))?;
        self.prepare_set(requests, cursor)?;
        let read = self
            .pool
            .reserve_target_set(owner, requests)
            .and_then(|pinned| pinned.try_read())
            .map_err(|error| frame(p, error))?;
        let canonical = (&mut self.operation[4 * PAGE_BYTES..5 * PAGE_BYTES])
            .try_into()
            .expect("canonical queue loan");
        let result = (|| {
            validate_set(storage, &read, canonical, p)?;
            let root_index = locate(storage, &read, requests[1], p)?;
            let root_bytes = read.bytes(root_index).map_err(|error| frame(p, error))?;
            let root = page::decode_page(root_bytes, requests[1].identity, PageKind::RetentionRoot)
                .map_err(|error| fatal(storage, p, Plan4PageCause::Page(error)))?;
            let cell = root
                .cell(0)
                .filter(|cell| cell.len() == 512)
                .ok_or_else(|| fatal(storage, p, Plan4PageCause::Invariant))?;
            if *seen_root {
                if cell != &root_cell[..] {
                    return Err(fatal(storage, p, Plan4PageCause::Invariant));
                }
            } else {
                root_cell.copy_from_slice(cell);
                *seen_root = true;
            }
            for request in &requests[2..] {
                let at = locate(storage, &read, *request, p)?;
                let bytes = read.bytes(at).map_err(|error| frame(p, error))?;
                let jobs = page::decode_page(bytes, request.identity, PageKind::RetentionJobs)
                    .map_err(|error| fatal(storage, p, Plan4PageCause::Page(error)))?;
                let expected_count = if request.identity.page.page_no == 6 {
                    4
                } else {
                    15
                };
                let ordinal = (request.identity.page.page_no - 2) * 15;
                if jobs.len() != expected_count || u64::from(*count) != ordinal {
                    return Err(fatal(storage, p, Plan4PageCause::Invariant));
                }
                for slot in 0..jobs.len() {
                    let cell = jobs
                        .cell(slot)
                        .filter(|cell| cell.len() == 1024)
                        .ok_or_else(|| fatal(storage, p, Plan4PageCause::Invariant))?;
                    // The canonical validator checked Free/gen1. Hash actual cells.
                    hash.update(cell);
                    *count = count
                        .checked_add(1)
                        .ok_or_else(|| fatal(storage, p, Plan4PageCause::Invariant))?;
                }
            }
            Ok(())
        })();
        finish(result, read.release_checked(), p)
    }
}
