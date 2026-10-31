//! Sealed original CREATE/pool bridge. Parsed fields cannot manufacture any of
//! these capabilities. Factories belong to the actual Reserve/Forward/CLR
//! producer and remain closed until its semantic preparation is connected.
#[path = "catalog_live.rs"]
pub(super) mod catalog_live;
pub(in crate::storage) use catalog_live::retirement::CatalogRetirementRejected;
#[path = "catalog_commit.rs"]
pub(super) mod catalog_commit;
#[cfg(test)]
pub(super) use catalog_commit::pending_window_parts as catalog_commit_pending_parts;
#[cfg(test)]
pub(super) use catalog_commit::phase_source_observations as catalog_commit_phase_observations;
#[cfg(test)]
pub(super) use catalog_commit::source_phases as catalog_commit_source_phases;
pub(super) use catalog_commit::writer_window_sources as catalog_commit_writer_windows;

use crate::{
    storage::{
        dirty_frame::{FrameStamp, TargetRequest, TxPairClaimKey},
        page::{PAGE_BYTES, PageLsn},
        session_fence::SessionFence,
        transaction_record::TxnRef,
        wal_io::WalRequirement,
    },
    transaction::concurrency::PageConcurrency,
};
use std::sync::atomic::AtomicU8;

/// The actual Forward producer lends its original control bank and its checked
/// after bytes to the whole-set installer. No external constructor is exposed.
/// This is a control effect, not a callback or a separately reconstructed owner.
pub(in crate::storage) struct TxPairControlCommit<'i, 'control, 'c, 's> {
    authority: &'i TxPairAuthority<'i, 'c, 's>,
    original: &'i mut super::control::RetentionLedger<'control, 'c, 's>,
    requirement: WalRequirement<'s>,
    transaction_slot: usize,
    job_ordinal: usize,
    transaction_before: [u8; 32],
    aging_before: [u8; 32],
    job_before: [u8; 32],
    inventory_before: [u8; 32],
    transaction_after: &'i [u8; 1536],
    aging_after: &'i [u8; 1536],
    job_after: &'i [u8; 1024],
    inventory_after: &'i [u8; 16384],
    applied: bool,
}
impl TxPairControlCommit<'_, '_, '_, '_> {
    pub(in crate::storage) fn apply(
        &mut self,
        key: &TxPairClaimKey<'_, '_>,
        requirement: WalRequirement<'_>,
    ) -> crate::storage::dirty_frame::Result<()> {
        use super::control::{self, ACTOR, AGING_SLOT, ATT_SLOTS, PHASE, STABLE, STAGED};
        use foundation::{codec::content_digest, error::Effect};
        // Scope2's private before-image loan replaces the old control-tail
        // before lane. Its original control tail now retains the XOI inventory.
        const INVENTORY: std::ops::Range<usize> = 131_072..147_456;
        let bad = self.applied
            || !key.matches_authority(self.authority)
            || self.transaction_slot >= ATT_SLOTS
            || self.job_ordinal >= 64
            || !std::ptr::eq(requirement.origin, self.requirement.origin)
            || requirement.exact != self.requirement.exact
            || !std::ptr::eq(self.authority.fence(), self.requirement.origin)
            || self.original.pending.is_none_or(|pending| {
                !std::ptr::eq(pending.origin, requirement.origin)
                    || pending.exact != requirement.exact
            });
        let bad = bad || {
            let row = self.original.row(self.transaction_slot.min(ATT_SLOTS - 1));
            row[PHASE] != STAGED
                || TxnRef::decode(&row[ACTOR]).ok() != Some(self.authority.actor())
                || content_digest(&row[..1536]) != self.transaction_before
                || content_digest(&self.original.row(AGING_SLOT)[..1536]) != self.aging_before
                || content_digest(&self.original.jobs()[self.job_ordinal.min(63)])
                    != self.job_before
                || content_digest(&self.original.bytes[INVENTORY]) != self.inventory_before
        };
        if bad {
            self.authority.fence().fence();
            return Err(crate::storage::dirty_frame::DirtyError {
                cause: crate::storage::dirty_frame::Cause::Invariant,
                effect: Effect::CallerMustResolve,
            });
        }
        self.original.row_mut(self.transaction_slot)[..1536]
            .copy_from_slice(self.transaction_after);
        self.original.row_mut(AGING_SLOT)[..1536].copy_from_slice(self.aging_after);
        control::jobs_mut(self.original.bytes)[self.job_ordinal].copy_from_slice(self.job_after);
        self.original.bytes[INVENTORY].copy_from_slice(self.inventory_after);
        self.original.row_mut(self.transaction_slot)[PHASE] = STABLE;
        self.applied = true;
        Ok(())
    }
}

pub(in crate::storage) struct TxPairAuthority<'i, 'c, 's> {
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    fence: &'s SessionFence,
    actor: TxnRef,
    reserve: PageLsn,
    existing: &'i [TargetRequest; 7],
    pair: &'i [TargetRequest; 2],
}
impl<'i, 'c, 's> TxPairAuthority<'i, 'c, 's> {
    /// Metadata-only pool fixtures; never an actual WAL/allocator authority.
    #[cfg(test)]
    pub(in crate::storage) fn metadata_fixture(
        core: &'c PageConcurrency<'s>,
        binding: &'s AtomicU8,
        fence: &'s SessionFence,
        actor: TxnRef,
        reserve: PageLsn,
        existing: &'i [TargetRequest; 7],
        pair: &'i [TargetRequest; 2],
    ) -> Self {
        Self {
            core,
            binding,
            fence,
            actor,
            reserve,
            existing,
            pair,
        }
    }

    pub(in crate::storage) fn core(&self) -> &'c PageConcurrency<'s> {
        self.core
    }
    pub(in crate::storage) fn binding(&self) -> &'s AtomicU8 {
        self.binding
    }
    pub(in crate::storage) fn fence(&self) -> &'s SessionFence {
        self.fence
    }
    pub(in crate::storage) fn actor(&self) -> TxnRef {
        self.actor
    }
    pub(in crate::storage) fn reserve(&self) -> PageLsn {
        self.reserve
    }
    pub(in crate::storage) fn existing(&self) -> &'i [TargetRequest; 7] {
        self.existing
    }
    pub(in crate::storage) fn pair(&self) -> &'i [TargetRequest; 2] {
        self.pair
    }
}

/// Actual immutable Forward image set after successful semantic stage. The
/// sorted nine targets include six existing changes, two INITs and one unchanged
/// CatalogAnchor proof. None before means INIT; None after means that proof.
pub(in crate::storage) struct TxPairInstallPermit<'i, 'c, 's> {
    key: &'i TxPairClaimKey<'c, 's>,
    requirement: WalRequirement<'s>,
    targets: &'i [TargetRequest; 9],
    before: &'i [Option<FrameStamp>; 9],
    after: &'i [Option<&'i [u8; PAGE_BYTES]>; 9],
}
impl<'i, 'c, 's> TxPairInstallPermit<'i, 'c, 's> {
    pub(in crate::storage) fn key(&self) -> &'i TxPairClaimKey<'c, 's> {
        self.key
    }
    pub(in crate::storage) fn requirement(&self) -> WalRequirement<'s> {
        self.requirement
    }
    pub(in crate::storage) fn targets(&self) -> &'i [TargetRequest; 9] {
        self.targets
    }
    pub(in crate::storage) fn before(&self) -> &'i [Option<FrameStamp>; 9] {
        self.before
    }
    pub(in crate::storage) fn after(&self) -> &'i [Option<&'i [u8; PAGE_BYTES]>; 9] {
        self.after
    }
}

/// Only an authentic original stage rejection may restore the pre-stage path.
pub(in crate::storage) struct TxPairStageRejection<'i, 'c, 's> {
    key: &'i TxPairClaimKey<'c, 's>,
}
impl<'i, 'c, 's> TxPairStageRejection<'i, 'c, 's> {
    pub(in crate::storage) fn key(&self) -> &'i TxPairClaimKey<'c, 's> {
        self.key
    }
}

/// The original selected DWB/flush owner seals this only while its mutable
/// bound excludes new capture. It proves absence for this exact physical pair,
/// not merely that the RAM DPT currently has no capture claim.
struct PairIoQuiescence<'q, 'c, 's, 'w> {
    original: &'q mut super::flush::RetentionFlushControl<'c, 's, 'w>,
    binding: &'s AtomicU8,
    fence: &'s SessionFence,
    selected_generation: u64,
    pair: [TargetRequest; 2],
}
impl PairIoQuiescence<'_, '_, '_, '_> {
    fn matches(&self, binding: &AtomicU8, fence: &SessionFence, pair: &[TargetRequest; 2]) -> bool {
        self.original.phase == super::flush::RetentionFlushPhase::Ready
            && std::ptr::eq(self.binding, binding)
            && std::ptr::eq(self.fence, fence)
            && self.selected_generation != 0
            && self.pair == *pair
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::storage) enum TxPairCancelKind {
    /// Original Reserve call never took effect; allocator generations are old.
    Unstaged,
    /// Actual Cancel was wholly adopted; reserved generations stay consumed.
    Cancelled,
}
pub(in crate::storage) struct TxPairCancelPermit<'i, 'c, 's, 'w> {
    authority: &'i TxPairAuthority<'i, 'c, 's>,
    io: PairIoQuiescence<'i, 'c, 's, 'w>,
    kind: TxPairCancelKind,
    requirement: Option<WalRequirement<'s>>,
    allocator_after: FrameStamp,
}
impl<'i, 'c, 's, 'w> TxPairCancelPermit<'i, 'c, 's, 'w> {
    pub(in crate::storage) fn authority(&self) -> &'i TxPairAuthority<'i, 'c, 's> {
        self.authority
    }
    pub(in crate::storage) fn kind(&self) -> TxPairCancelKind {
        self.kind
    }
    pub(in crate::storage) fn requirement(&self) -> Option<WalRequirement<'s>> {
        self.requirement
    }
    pub(in crate::storage) fn allocator_after(&self) -> FrameStamp {
        self.allocator_after
    }
    pub(in crate::storage) fn io_matches(&self, binding: &AtomicU8, fence: &SessionFence) -> bool {
        self.io.matches(binding, fence, self.authority.pair())
    }
}

/// Issued only after the actual CLR whole semantic installation, before any
/// new allocation is exposed. The pair itself is no longer in RootJob debt.
pub(in crate::storage) struct TxPairRetirePermit<'i, 'c, 's, 'w> {
    core: &'c PageConcurrency<'s>,
    actor: TxnRef,
    reserve: PageLsn,
    io: PairIoQuiescence<'i, 'c, 's, 'w>,
    requirement: WalRequirement<'s>,
    allocator_after: FrameStamp,
    before: [Option<FrameStamp>; 2],
}
impl<'c, 's> TxPairRetirePermit<'_, 'c, 's, '_> {
    pub(in crate::storage) fn core(&self) -> &'c PageConcurrency<'s> {
        self.core
    }
    pub(in crate::storage) fn actor(&self) -> TxnRef {
        self.actor
    }
    pub(in crate::storage) fn reserve(&self) -> PageLsn {
        self.reserve
    }
    pub(in crate::storage) fn pair(&self) -> &[TargetRequest; 2] {
        &self.io.pair
    }
    pub(in crate::storage) fn before(&self) -> &[Option<FrameStamp>; 2] {
        &self.before
    }
    pub(in crate::storage) fn requirement(&self) -> WalRequirement<'s> {
        self.requirement
    }
    pub(in crate::storage) fn allocator_after(&self) -> FrameStamp {
        self.allocator_after
    }
    pub(in crate::storage) fn io_matches(&self, binding: &AtomicU8, fence: &SessionFence) -> bool {
        self.io.matches(binding, fence, &self.io.pair)
    }
}

#[cfg(test)]
pub(super) use catalog_commit::wait_source_observation as catalog_wait_source_observation;
