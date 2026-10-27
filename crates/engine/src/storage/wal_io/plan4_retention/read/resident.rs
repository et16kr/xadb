//! Resident-only location observations. The original allocator image is copied
//! to the caller loan, never to a new owned page or a second directory. No WAL
//! replay, durability wait, reader file, or LOAD is reachable from this module.
use super::*;
use crate::storage::{
    page::PageRef,
    status_home,
    transaction_record::{StatusSlotRef, TransactionStatusCell, TxnRef},
    undo_ref::UndoRef,
};

/// A normal eviction or allocator replacement is retryable for this class.
/// Corruption/poison/fence retain their original cause and strong effect.
pub(super) fn retry(error: RetentionError) -> RetentionError {
    if error.effect == Effect::Unchanged
        && matches!(
            error.cause,
            RetentionCause::NotResident
                | RetentionCause::Frame(dirty_frame::DirtyError {
                    cause: dirty_frame::Cause::MissingTarget,
                    ..
                })
        )
    {
        unchanged(RetentionPhase::Census, RetentionCause::Busy)
    } else {
        error
    }
}

/// Once the exact source was validated, its original reference pin (or fixed
/// Catalog root identity) forbids normal remapping. A missing resident frame
/// can still be evicted, but a current allocator mismatch is corruption.
pub(super) fn selected_error<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    error: RetentionError,
) -> RetentionError {
    if error.effect == Effect::Unchanged && matches!(error.cause, RetentionCause::Membership) {
        fatal(storage, RetentionPhase::Census, RetentionCause::Invariant)
    } else {
        retry(error)
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    ReadPort<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    fn copy_resident_allocator(
        &self,
        owner: &mut Owner<'c, 's>,
        copy: &mut [u8; PAGE_BYTES],
    ) -> Result<(TargetRequest, FrameStamp)> {
        let allocator = self
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Census, e))?;
        let guard = self
            .pool
            .reserve_target_set(owner, &[allocator])
            .and_then(|pins| pins.try_read())
            .map_err(|e| retry(frame(RetentionPhase::Census, e)))?;
        let copied = (|| {
            let stamp = guard
                .observe(0)
                .map_err(|e| frame(RetentionPhase::Census, e))?
                .frame;
            copy.copy_from_slice(
                guard
                    .bytes(0)
                    .map_err(|e| frame(RetentionPhase::Census, e))?,
            );
            Ok((allocator, stamp))
        })();
        let copied = merge(
            copied,
            guard
                .release_checked()
                .map_err(|e| frame(RetentionPhase::Census, e)),
        )?;
        #[cfg(test)]
        crate::transaction::concurrency::catalog_test_support::hit(
            self.pool.bound_core(),
            crate::transaction::concurrency::catalog_test_support::Point::ResidentAllocatorCopied,
        );
        Ok(copied)
    }

    fn check_resident_allocator(
        &self,
        owner: &mut Owner<'c, 's>,
        allocator: TargetRequest,
        stamp: FrameStamp,
    ) -> Result<()> {
        let guard = self
            .pool
            .reserve_target_set(owner, &[allocator])
            .and_then(|pins| pins.try_read())
            .map_err(|e| retry(frame(RetentionPhase::Census, e)))?;
        let same = guard
            .observe(0)
            .map_err(|e| frame(RetentionPhase::Census, e))
            .and_then(|now| {
                if now.frame == stamp {
                    Ok(())
                } else {
                    Err(unchanged(RetentionPhase::Census, RetentionCause::Busy))
                }
            });
        merge(
            same,
            guard
                .release_checked()
                .map_err(|e| frame(RetentionPhase::Census, e)),
        )
    }

    fn resident_allocation<'i>(
        &self,
        copy: &'i [u8; PAGE_BYTES],
        allocator: TargetRequest,
    ) -> Result<allocation_page::AllocationPage<'i>> {
        allocation_page::decode_allocation_page(
            copy,
            allocator.identity,
            self.storage.profile().spec().namespace,
        )
        .map_err(|e| {
            fatal(
                self.storage,
                RetentionPhase::Census,
                RetentionCause::Allocation(e),
            )
        })
    }

    pub(super) fn resident_status_location(
        &self,
        owner: &mut Owner<'c, 's>,
        reference: TxnRef,
        copy: &mut [u8; PAGE_BYTES],
    ) -> Result<(TargetRequest, StatusSlotRef)> {
        let (allocator, stamp) = self.copy_resident_allocator(owner, copy)?;
        let allocation = self.resident_allocation(copy, allocator)?;
        let mut found = None;
        for index in 1..64 {
            let entry = allocation.entry(index).map_err(|e| {
                fatal(
                    self.storage,
                    RetentionPhase::Census,
                    RetentionCause::Allocation(e),
                )
            })?;
            let Some(request) = allocated_request(allocator, index, entry) else {
                continue;
            };
            if request.kind != PageKind::TransactionStatus {
                continue;
            }
            // Even after finding the reference, a missing later Status page
            // prevents a uniqueness claim. Never return the first partial hit.
            if self
                .pool
                .resident_request(request)
                .map_err(|e| frame(RetentionPhase::Census, e))?
                .is_none()
            {
                return Err(unchanged(RetentionPhase::Census, RetentionCause::Busy));
            }
            let selected = (|| {
                let guard = self.read(owner, &Plan4Target { request })?;
                let selected = (|| {
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
                    for slot in 0..page.len() {
                        let home = page.cell(slot).map_err(|e| {
                            fatal(
                                self.storage,
                                RetentionPhase::Census,
                                RetentionCause::Page(e),
                            )
                        })?;
                        if matches!(home.core(), TransactionStatusCell::Occupied(status) if status.reference() == reference)
                        {
                            let location = StatusSlotRef::new(
                                request.identity.page,
                                slot as u16,
                                reference.generation(),
                            )
                            .map_err(|e| {
                                fatal(
                                    self.storage,
                                    RetentionPhase::Census,
                                    RetentionCause::Page(e),
                                )
                            })?;
                            if found.replace((request, location)).is_some() {
                                return Err(fatal(
                                    self.storage,
                                    RetentionPhase::Census,
                                    RetentionCause::Invariant,
                                ));
                            }
                        }
                    }
                    Ok(())
                })();
                merge(selected, guard.release_checked())
            })();
            if let Err(error) = selected {
                // The request came from a copied allocator. Recheck after
                // the physical read's borrow ends before calling it corrupt.
                if error.effect == Effect::Unchanged
                    && matches!(error.cause, RetentionCause::Membership)
                {
                    self.check_resident_allocator(owner, allocator, stamp)?;
                    return Err(fatal(
                        self.storage,
                        RetentionPhase::Census,
                        RetentionCause::Invariant,
                    ));
                }
                return Err(retry(error));
            }
        }
        self.check_resident_allocator(owner, allocator, stamp)?;
        found.ok_or_else(|| {
            fatal(
                self.storage,
                RetentionPhase::Census,
                RetentionCause::Invariant,
            )
        })
    }

    pub(super) fn resident_undo_location(
        &self,
        owner: &mut Owner<'c, 's>,
        reference: UndoRef,
        copy: &mut [u8; PAGE_BYTES],
    ) -> Result<TargetRequest> {
        let (allocator, stamp) = self.copy_resident_allocator(owner, copy)?;
        let allocation = self.resident_allocation(copy, allocator)?;
        let index = usize::try_from(reference.page().page_no)
            .ok()
            .filter(|i| (1..64).contains(i));
        let request = match index {
            Some(index) => allocated_request(
                allocator,
                index,
                allocation.entry(index).map_err(|e| {
                    fatal(
                        self.storage,
                        RetentionPhase::Census,
                        RetentionCause::Allocation(e),
                    )
                })?,
            ),
            None => None,
        };
        self.check_resident_allocator(owner, allocator, stamp)?;
        let request = request
            .filter(|r| r.kind == PageKind::Undo && r.identity.page == reference.page())
            .ok_or_else(|| {
                fatal(
                    self.storage,
                    RetentionPhase::Census,
                    RetentionCause::Invariant,
                )
            })?;
        if self
            .pool
            .resident_request(request)
            .map_err(|e| frame(RetentionPhase::Census, e))?
            .is_none()
        {
            return Err(unchanged(RetentionPhase::Census, RetentionCause::Busy));
        }
        Ok(request)
    }
}

/// A copied request is only an observation. The original allocator/target pair
/// is pinned and membership-checked again by read/read_catalog_undo.
fn allocated_request(
    allocator: TargetRequest,
    index: usize,
    entry: AllocationEntry,
) -> Option<TargetRequest> {
    let AllocationEntry::Allocated {
        generation,
        object,
        object_generation,
        kind,
    } = entry
    else {
        return None;
    };
    Some(TargetRequest {
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
    })
}

pub(super) fn source_bytes() -> usize {
    use std::mem::size_of;
    // The fixed allocator copy is in the existing caller loan. Copy, scan and
    // final stamp verification are sequential, while the copied view/request,
    // found result and original stamp survive the nested physical read.
    super::catalog_reference_read_source_bytes()
        + 2 * size_of::<allocation_page::AllocationPage<'_>>()
        + 2 * size_of::<status_home::StatusHomePage<'_>>()
        + 2 * size_of::<status_home::StatusHome>()
        + 2 * size_of::<TransactionStatusCell>()
        + 4 * size_of::<TargetRequest>()
        + 3 * size_of::<FrameStamp>()
        + 3 * size_of::<Option<(TargetRequest, StatusSlotRef)>>()
        + 3 * size_of::<Result<(TargetRequest, StatusSlotRef)>>()
        + 4 * size_of::<Result<()>>()
        + 2 * size_of::<Result<allocation_page::AllocationPage<'_>>>()
        + 2 * size_of::<Result<AllocationEntry>>()
        + 16 * size_of::<&()>()
        + 16 * size_of::<usize>()
}
