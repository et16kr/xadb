//! Pending-Commit-compatible reader LOAD over the original pool and history.
//! The private permit is issued only after an installed source cut and both
//! selected physical home reads have been verified with all page gates absent.
use super::*;
use crate::transaction::concurrency::OwnerIdentity;
use crate::{
    api::Namespace,
    storage::{
        allocation_page::{self, AllocationEntry},
        dirty_frame::{LoadClaimIdentity, LoadClaimStep, RetentionLoadClaim},
    },
};

pub(in crate::storage) struct VerifiedLookupLoad<'a, 'c, 's> {
    identity: LoadClaimIdentity<'c, 's>,
    bytes: &'a [u8; PAGE_BYTES],
    allocator: PageIdentity,
    namespace: Namespace,
}

/// This outer scope is private and remains in the actual resolver until both
/// audits finish. Forgetting an inner pool claim cannot return a usable Owner.
struct CatalogLookupOp<'a, 'b, 'c, 's, 'p, 'd> {
    owner: &'a mut Owner<'c, 's>,
    pool: &'b SharedPool<'c, 's, 'p, 'd>,
    identity: OwnerIdentity,
    load_attempt: Option<u64>,
    completed: bool,
}
impl<'a, 'b, 'c, 's, 'p, 'd> CatalogLookupOp<'a, 'b, 'c, 's, 'p, 'd> {
    fn new(owner: &'a mut Owner<'c, 's>, pool: &'b SharedPool<'c, 's, 'p, 'd>) -> Result<Self> {
        pool.check_retention_reader(owner)
            .map_err(|error| frame(RetentionPhase::Load, error))?;
        let identity = owner
            .visibility_identity(pool.bound_core())
            .map_err(|error| RetentionError {
                phase: RetentionPhase::Load,
                cause: RetentionCause::Concurrency(error),
                effect: error.effect,
            })?;
        Ok(Self {
            owner,
            pool,
            identity,
            load_attempt: None,
            completed: false,
        })
    }
    fn finish_checked(mut self) -> Result<()> {
        let result = self
            .pool
            .audit_load_released(self.identity, self.load_attempt)
            .map_err(|error| frame(RetentionPhase::Load, error));
        self.completed = true;
        result
    }
}
impl Drop for CatalogLookupOp<'_, '_, '_, '_, '_, '_> {
    fn drop(&mut self) {
        if !self.completed {
            // The pool audit fences on a forgotten same-owner claim. It does
            // not erase that claim or reconstruct the original owner quota.
            let _ = self
                .pool
                .audit_load_released(self.identity, self.load_attempt);
        }
    }
}

fn page_io<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    error: crate::storage::page_io::PageIoError,
) -> RetentionError {
    if matches!(error, crate::storage::page_io::PageIoError::Busy) {
        unchanged(RetentionPhase::Load, RetentionCause::Busy)
    } else {
        fatal(storage, RetentionPhase::Load, RetentionCause::PageIo(error))
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    read::RetentionReader<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    /// A finite original-pool residency step. The caller's copied Catalog and
    /// transient Txn/Undo refs live outside `scratch`; this prefix uses six
    /// pages and never touches the Catalog copy in the lookup loan's tail.
    pub(super) fn load_lookup_target(
        &self,
        owner: &mut Owner<'c, 's>,
        request: TargetRequest,
        cursor: &mut Plan4ResidencyCursor,
        protected: &[TargetRequest],
        scratch: &mut [u8],
    ) -> Result<RetentionResidencyStep> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        if scratch.len() < 6 * PAGE_BYTES {
            return Err(unchanged(RetentionPhase::Load, RetentionCause::Capacity));
        }
        let mut outer = CatalogLookupOp::new(owner, self.pool)?;
        // This creates only the separate attempt audit. Checkout and installed
        // source capture happen after the successful physical target claim.
        let mut replay_audit = self.history.replay_scope(outer.owner)?;
        let result = (|| {
            let step = self
                .pool
                .claim_retention_load(outer.owner, request, cursor.next, protected)
                .map_err(|error| frame(RetentionPhase::Load, error))?;
            match step {
                LoadClaimStep::Resident(candidate) => Ok(RetentionResidencyStep::Resident {
                    index: candidate.index,
                    frame: candidate.frame.ok_or_else(|| {
                        fatal(storage, RetentionPhase::Load, RetentionCause::Invariant)
                    })?,
                    loaded: false,
                }),
                LoadClaimStep::FlushRequired(candidate) => {
                    let frame = candidate.frame.ok_or_else(|| {
                        fatal(storage, RetentionPhase::Load, RetentionCause::Invariant)
                    })?;
                    let request = self
                        .pool
                        .target_request(candidate.index)
                        .map_err(frame_error)?;
                    Ok(RetentionResidencyStep::FlushRequired {
                        target: Plan4Target { request },
                        rec_lsn: frame.rec_lsn.ok_or_else(|| {
                            fatal(storage, RetentionPhase::Load, RetentionCause::Invariant)
                        })?,
                    })
                }
                LoadClaimStep::Claimed(mut claim) => {
                    let identity = claim.identity();
                    // Record before any fallible action or token can escape.
                    outer.load_attempt = Some(identity.attempt());
                    let loaded = self.resolve_lookup_claim(&mut claim, &mut replay_audit, scratch);
                    let loaded = match loaded {
                        Ok(stamp) => {
                            drop(claim);
                            Ok(stamp)
                        }
                        Err(error) if error.effect == Effect::Unchanged => {
                            let cleanup = claim.cancel().map_err(frame_error);
                            merge(Err(error), cleanup)
                        }
                        Err(error) => {
                            drop(claim);
                            Err(error)
                        }
                    }?;
                    cursor.next = identity.index() + 1;
                    Ok(RetentionResidencyStep::Resident {
                        index: identity.index(),
                        frame: loaded,
                        loaded: true,
                    })
                }
            }
        })();
        // The independent replay attempt must be empty even when the pool
        // claim was already canceled successfully. Neither number substitutes
        // for the other registry's checked completion.
        let result = merge(result, replay_audit.finish_checked());
        merge(result, outer.finish_checked())
    }

    fn resolve_lookup_claim(
        &self,
        claim: &mut RetentionLoadClaim<'_, '_, 'c, 's, 'p, 'd>,
        audit: &mut history::runtime::ReplayScope<'_, 'a, 'c, 's>,
        scratch: &mut [u8],
    ) -> Result<FrameStamp> {
        let storage = self.storage;
        let identity = claim.identity();
        let request = identity.request();
        let page_no = usize::try_from(request.identity.page.page_no)
            .map_err(|_| unchanged(RetentionPhase::Load, RetentionCause::Membership))?;
        if !(1..64).contains(&page_no) {
            return Err(unchanged(RetentionPhase::Load, RetentionCause::Membership));
        }
        let pages: &mut [[u8; PAGE_BYTES]; 6] = scratch[..6 * PAGE_BYTES]
            .as_chunks_mut::<PAGE_BYTES>()
            .0
            .try_into()
            .expect("six caller pages");
        let [
            canonical,
            allocator,
            first,
            second,
            filemap_root,
            filemap_chunk,
        ] = pages;
        claim.capture_victim().map_err(frame_error)?;
        let allocator_stamp = claim.copy_allocator(allocator).map_err(frame_error)?;
        let cut = self.history.source_cut()?;
        let durable = cut.ensure_durable(storage)?;
        let mut token = audit.checkout()?;
        let copied = (|| {
            history::replay_lookup_cut(storage, &cut, durable.as_ref(), token.bytes_mut())?;
            let replay = token.bytes();
            let image = history::arena_page(replay, page_no);
            let page = crate::storage::page::decode_page(image, request.identity, request.kind)
                .map_err(|error| {
                    fatal(storage, RetentionPhase::Load, RetentionCause::Page(error))
                })?;
            if page.header().level != request.level {
                return Err(fatal(
                    storage,
                    RetentionPhase::Load,
                    RetentionCause::Invariant,
                ));
            }
            // Both the original replay allocator and current pool entry must
            // identify the claimed target. No install permit exists yet.
            let namespace = storage.profile().spec().namespace;
            if !target_membership(
                history::arena_page(replay, 0),
                allocator_stamp.identity,
                namespace,
                request,
            ) || !target_membership(allocator, allocator_stamp.identity, namespace, request)
            {
                return Err(fatal(
                    storage,
                    RetentionPhase::Load,
                    RetentionCause::Invariant,
                ));
            }
            canonical.copy_from_slice(image);
            Ok(())
        })();
        merge(copied, token.return_checked())?;
        cut.unchanged()?;
        for output in [first, &mut *second] {
            let mut reader = storage
                .core
                .open_reader(filemap_root, filemap_chunk)
                .map_err(|error| page_io(storage, error))?;
            reader
                .read_retention_home(
                    request.identity,
                    request.kind,
                    request.level,
                    allocator_stamp.identity,
                    output,
                )
                .map_err(|error| page_io(storage, error))?;
            if output != canonical {
                return Err(fatal(
                    storage,
                    RetentionPhase::Load,
                    RetentionCause::Invariant,
                ));
            }
        }
        cut.unchanged()?;
        let permit = VerifiedLookupLoad {
            identity,
            bytes: second,
            allocator: allocator_stamp.identity,
            namespace: storage.profile().spec().namespace,
        };
        claim.install(&permit).map_err(frame_error)
    }
}

fn frame_error(error: dirty_frame::DirtyError) -> RetentionError {
    frame(RetentionPhase::Load, error)
}
impl<'a, 'c, 's> VerifiedLookupLoad<'a, 'c, 's> {
    pub(in crate::storage) fn identity(&self) -> LoadClaimIdentity<'c, 's> {
        self.identity
    }
    pub(in crate::storage) fn request(&self) -> TargetRequest {
        self.identity.request()
    }
    pub(in crate::storage) fn bytes(&self) -> &'a [u8; PAGE_BYTES] {
        self.bytes
    }

    /// Other allocator entries may have advanced normally while this target
    /// was claimed. Only its exact original identity must stay allocated.
    pub(in crate::storage) fn accepts_current_allocator(&self, bytes: &[u8; PAGE_BYTES]) -> bool {
        target_membership(bytes, self.allocator, self.namespace, self.request())
    }
}

fn target_membership(
    bytes: &[u8; PAGE_BYTES],
    allocator: PageIdentity,
    namespace: Namespace,
    request: TargetRequest,
) -> bool {
    let page_no = request.identity.page.page_no;
    if !(1..64).contains(&page_no)
        || request.identity.database != namespace.database
        || request.identity.page.file != allocator.page.file
        || request.level != 0
    {
        return false;
    }
    let Ok(allocation) = allocation_page::decode_allocation_page(bytes, allocator, namespace)
    else {
        return false;
    };
    allocation.entry(page_no as usize).ok()
        == Some(AllocationEntry::Allocated {
            generation: request.identity.page.incarnation,
            object: request.identity.owner,
            object_generation: request.identity.owner_generation,
            kind: request.kind,
        })
}

/// Actual loader carriers; caller retains the lookup's copied Catalog and
/// semantic reference pins separately. Page/file-map backing is borrowed.
pub(super) fn source_bytes<W: WriterNamespace>() -> usize {
    use crate::storage::{
        dirty_frame::LoadClaimStep,
        page::{PageHeader, PageView},
    };
    use std::mem::size_of;
    let leaf = dirty_frame::lookup_load_census();
    let outer = size_of::<CatalogLookupOp<'_, '_, '_, '_, '_, '_>>()
        + size_of::<TargetRequest>()
        + size_of::<history::runtime::ReplayScope<'_, '_, '_, '_>>()
        + 3 * size_of::<Result<RetentionResidencyStep>>()
        + 3 * size_of::<Result<()>>()
        + size_of::<(
            &mut Owner<'_, '_>,
            &Plan4ResidencyCursor,
            &[TargetRequest],
            &mut [u8],
        )>();
    // claim_retention_load returns before its linear claim is moved into the
    // resolving arm. The wide selection Result/enum is not a second live
    // claim. Retain it during selection/return, and the sole claim plus its
    // actual copied identities during resolution/cancellation instead.
    let selecting = 2 * size_of::<dirty_frame::Result<LoadClaimStep<'_, '_, '_, '_, '_, '_>>>()
        + size_of::<LoadClaimStep<'_, '_, '_, '_, '_, '_>>()
        + leaf.selection_nested;
    let claimed = size_of::<RetentionLoadClaim<'_, '_, '_, '_, '_, '_>>()
        + 2 * size_of::<LoadClaimIdentity<'_, '_>>();
    // `loaded` and its two Result/match return carriers exist only after the
    // resolver returns. The resolver's still-live allocator stamp is charged
    // in `resolving`; final claim.install is already in that nested maximum.
    let returning = claimed
        + 3 * size_of::<FrameStamp>()
        + 2 * size_of::<Result<FrameStamp>>()
        + leaf.install_nested.max(leaf.cancellation_nested);
    let resolving = size_of::<TargetRequest>()
        + size_of::<[&mut [u8; PAGE_BYTES]; 6]>()
        + size_of::<FrameStamp>()
        + size_of::<history::runtime::LookupSourceCut<'_, '_, '_, '_>>()
        + size_of::<Option<DurableWal<'_>>>()
        + size_of::<history::runtime::ReplayLoan<'_, '_, '_, '_>>()
        + 2 * size_of::<Result<()>>()
        + size_of::<(usize, Namespace, &MutableStorageOwner<'_, '_, W>)>();
    // Replay returns before the copied-page validation views are declared.
    // Those end with the copied closure before home reads; the final permit
    // is constructed after both reads. Keep their sum in the later phase.
    let after_replay = size_of::<VerifiedLookupLoad<'_, '_, '_>>()
        + size_of::<PageView<'_>>()
        + size_of::<PageHeader>()
        + 2 * size_of::<allocation_page::AllocationPage<'_>>()
        + 2 * size_of::<TargetRequest>();
    outer
        + selecting
            .max(returning)
            .max(claimed + leaf.cancellation_nested)
            .max(
                claimed
                    + resolving
                    + history::source_bytes::<W>()
                        .peak
                        .max(
                            history::runtime::lookup_source_bytes()
                                + crate::storage::wal_io::DURABILITY_CALL_BYTES,
                        )
                        .max(
                            after_replay
                                + crate::storage::page_io::retention_home_source_bytes::<W>()
                                    .max(leaf.copy_nested)
                                    .max(leaf.install_nested),
                        ),
            )
}
