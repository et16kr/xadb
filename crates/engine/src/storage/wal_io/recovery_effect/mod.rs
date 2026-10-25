//! Closed, effect-owning recovery of the current genesis grammar. No READY.
#![allow(dead_code)] // Recovery is intentionally not a general open API.
use super::*;
use crate::storage::{page, session_fence::SessionFence};
use foundation::io::{FileIo, sync, write_all_at};
use std::mem::size_of;

#[path = "bootstrap.rs"]
mod boot_io;
mod physical;
mod port_io;
mod resources;
mod selection;
mod types;
pub(crate) use types::*;

pub(crate) const MAINTENANCE_BYTES: usize = 12_582_912;
pub(crate) const FLUSH_BYTES: usize = 4_194_304;
const HOME_A: usize = 0;
const PHYSICAL_BYTES: usize = TARGET_COUNT * PAGE_BYTES;
const SAVED_TARGETS: usize = PHYSICAL_BYTES;
const SLOT: usize = 65_536;
const CONTROL: usize = 98_304;
const SELECTOR_BYTES_AT: usize = 114_688;
const ROOT: usize = 131_072;
const MAP_WORK: usize = 147_456;
const ZERO_CHUNK: usize = 65_536;
const SLOT_NAMES: [&str; 2] = ["d3-rebase-0", "d3-rebase-1"];
const HOME_NAME: &str = "d3-data-0000000100000001";
const SLOT_CAP: u64 = 1_048_576;
type Result<T> = std::result::Result<T, RecoveryError>;

pub(crate) struct SourceCensus {
    pub(crate) outer: usize,
    pub(crate) prefix_nested: usize,
    pub(crate) inventory: usize,
    pub(crate) entry_phase: usize,
    pub(crate) return_phase: usize,
    pub(crate) work_nested: usize,
    pub(crate) peak: usize,
}
struct LocalAllowance {
    busy: bool,
    peak: u8,
}
impl LocalAllowance {
    fn enter(&mut self, phase: RecoveryPhase) -> Result<()> {
        if self.busy {
            return Err(fail(phase, RecoveryCause::Invariant));
        }
        self.busy = true;
        self.peak = 5;
        Ok(())
    }
    fn leave(&mut self) {
        self.busy = false;
    }
}
/// File/lease fields in Prefix drop before the allowance and arena borrows.
/// Every entry consumes its owner. A callback panic unwinds its local File first
/// and then the complete owner; no reusable allowance/retry owner survives it.
/// Normal errors close the File before returning its single allowance. Drop
/// performs no explicit database I/O.
pub(crate) struct ClosedRecovery<'a, 'd, W: WriterNamespace> {
    prefix: RecoveryPrefix<'a, W>,
    directory: &'d dyn DirectoryPort<Writer = W>,
    maintenance_tail: &'a mut [u8],
    flush: &'a mut [u8; FLUSH_BYTES],
    observation: RecoveryObservation,
    allowance: LocalAllowance,
    fence: SessionFence,
    source_seal: Option<SourceSeal>,
    final_allocator_hash: [u8; 32],
    final_descriptor_hash: Option<[u8; 32]>,
    final_transaction_hash: [u8; 32],
    slots: Option<SlotsProvisioned>,
    restored: Option<RestoredSeal>,
    reservation: Option<RecoveryReservation>,
}
struct RecoveryTarget {
    identity: PageIdentity,
    kind: PageKind,
    lsn: u64,
    digest: [u8; 32],
}
struct SlotsProvisioned {
    stamps: [FileMetadata; 2],
}
// Exclusive W and full arena borrows exclude a competing spender. This
// private, non-Clone guard owns fixed namespace growth and the verified source
// obligation; observations are diagnostic copies, never spend/release handles.
struct RecoveryReservation {
    owned_peak: u64,
    protected: u64,
    remaining: Option<RemainingBudget>,
    topup: u64,
    last_batch: u64,
}
struct RestoredSeal {
    lsn: u64,
    descriptor: Option<[u8; 32]>,
    transactions: [u8; 32],
    remaining: RemainingBudget,
    topup: u64,
}
struct SourceSeal {
    identity: FileIdentity,
    length: u64,
    complete_offset: u64,
    lsn: u64,
}
pub(crate) struct RecoveryInspection<'a, 'd, W: WriterNamespace> {
    owner: ClosedRecovery<'a, 'd, W>,
}
pub(crate) struct RecoveryFailure<'a, 'd, W: WriterNamespace> {
    owner: ClosedRecovery<'a, 'd, W>,
    error: RecoveryError,
}
impl<W: WriterNamespace> std::fmt::Debug for RecoveryFailure<'_, '_, W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}
fn fail(phase: RecoveryPhase, cause: RecoveryCause) -> RecoveryError {
    RecoveryError {
        phase,
        cause,
        effect: Effect::CallerMustResolve,
        lease_effect: LeaseEffect::MayHaveChanged,
    }
}
fn overflow(phase: RecoveryPhase) -> RecoveryError {
    fail(phase, RecoveryCause::ArithmeticOverflow)
}
fn filename(path: &str) -> FileName {
    FileName::new(path).expect("fixed recovery name")
}

// Two explicit entries expand the identical body with a closed internal scope.
// No runtime wrapper or additional wide owner/result return layer is introduced.
macro_rules! acquire_entry {
    ($method:ident, $scope:expr) => {
        impl<'a, 'd, W: WriterNamespace> ClosedRecovery<'a, 'd, W> {
            // The error intentionally carries fixed observations without a late heap
            // allocation. Its full value and result moves are in resources::source_census.
            #[allow(clippy::result_large_err)]
            pub(crate) fn $method<D: DirectoryPort<Writer = W>>(
                directory: &'d D,
                expected: Option<Namespace>,
                maintenance: &'a mut [u8; MAINTENANCE_BYTES],
                flush: &'a mut [u8; FLUSH_BYTES],
            ) -> std::result::Result<Self, RecoveryAcquireError> {
                let census = resources::source_census::<W>();
                if !resources::fits::<W>(&census) {
                    return Err(RecoveryAcquireError {
                        error: RecoveryError {
                            phase: RecoveryPhase::Preflight,
                            cause: RecoveryCause::Capacity,
                            effect: Effect::Unchanged,
                            lease_effect: LeaseEffect::NotAcquired,
                        },
                        effects: RecoveryEffects::default(),
                    });
                }
                if expected.is_some_and(|n| n.timeline == 0) {
                    return Err(RecoveryAcquireError {
                        error: RecoveryError {
                            phase: RecoveryPhase::Preflight,
                            cause: RecoveryCause::Control(ControlError::InvalidInput),
                            effect: Effect::Unchanged,
                            lease_effect: LeaseEffect::NotAcquired,
                        },
                        effects: RecoveryEffects::default(),
                    });
                }
                let (work, rest) = maintenance.split_at_mut(WORKSPACE_BYTES);
                let (transaction_arena, tail) = rest.split_at_mut(transactions::ARENA_BYTES);
                let transaction_arena = transaction_arena
                    .try_into()
                    .expect("canonical transaction arena");
                let work: &mut [u8; WORKSPACE_BYTES] = work.try_into().expect("fixed prefix split");
                let mut acquired = false;
                let prefix = RecoveryPrefix::acquire_for_effect(
                    directory,
                    expected,
                    work,
                    transaction_arena,
                    &mut acquired,
                    $scope,
                )
                .map_err(|cause| {
                    let effect = if acquired {
                        Effect::CallerMustResolve
                    } else {
                        cause.effect
                    };
                    let lease_effect = if acquired || effect == Effect::CallerMustResolve {
                        LeaseEffect::MayHaveChanged
                    } else {
                        LeaseEffect::NotAcquired
                    };
                    let phase = if cause.phase == super::Phase::Lease {
                        RecoveryPhase::Lease
                    } else {
                        RecoveryPhase::Prefix
                    };
                    RecoveryAcquireError {
                        error: RecoveryError {
                            phase,
                            cause: RecoveryCause::Prefix(cause),
                            effect,
                            lease_effect,
                        },
                        effects: RecoveryEffects::default(),
                    }
                })?;
                let known = prefix.observation.doublewrite;
                let batch = prefix.observation.dwb_header.batch_id;
                let state = prefix.observation.dwb_header.state;
                let mut owner = Self {
                    prefix,
                    directory,
                    maintenance_tail: tail,
                    flush,
                    observation: RecoveryObservation {
                        phase: RecoveryPhase::Inventory,
                        lease_effect: LeaseEffect::MayHaveChanged,
                        source_stabilized: false,
                        known,
                        possible: [Some(known), None],
                        batch,
                        state,
                        boot: BootObservation::default(),
                        ledger: LedgerObservation::default(),
                        physical: PhysicalObservation::default(),
                        effects: RecoveryEffects::default(),
                        fd_peak: 5,
                        source_census: census,
                    },
                    allowance: LocalAllowance {
                        busy: false,
                        peak: 5,
                    },
                    fence: SessionFence::new(),
                    source_seal: None,
                    final_allocator_hash: [0; 32],
                    final_descriptor_hash: None,
                    final_transaction_hash: [0; 32],
                    slots: None,
                    restored: None,
                    reservation: None,
                };
                if let Err(error) = owner.acquire_preflight() {
                    let effects = std::mem::take(&mut owner.observation.effects);
                    return Err(RecoveryAcquireError { error, effects });
                }
                Ok(owner)
            }
        }
    };
}

acquire_entry!(acquire, RecoveryScope::Effects);
acquire_entry!(acquire_mixed, RecoveryScope::MixedEffects);

impl<'a, 'd, W: WriterNamespace> ClosedRecovery<'a, 'd, W> {
    // Failure retains W, the source File and both complete arena loans. Boxing
    // on an I/O failure would add an unreserved allocation and weaken this owner.
    #[allow(clippy::result_large_err)]
    pub(crate) fn recover(
        mut self,
    ) -> std::result::Result<RecoveryInspection<'a, 'd, W>, RecoveryFailure<'a, 'd, W>> {
        let result = self.run_recovery();
        match result {
            Ok(()) => Ok(RecoveryInspection { owner: self }),
            Err(error) => {
                self.fence.fence();
                Err(RecoveryFailure { owner: self, error })
            }
        }
    }
    // Borrowed work ends before a wide success/failure owner is constructed.
    // This boundary is part of the source-value census, not a stack ABI claim.
    fn run_recovery(&mut self) -> Result<()> {
        self.provision()?;
        self.stabilize_source()?;
        self.repair_pending()?;
        self.restore_once()?;
        self.redo()?;
        self.flush_dirty()?;
        self.verify()
    }
    pub(crate) fn observation(&self) -> &RecoveryObservation {
        &self.observation
    }
    pub(crate) fn prefix(&self) -> &RecoveryPrefixObservation {
        self.prefix.observation()
    }
    pub(crate) fn effects(&self) -> &RecoveryEffects {
        &self.observation.effects
    }
}
impl<W: WriterNamespace> RecoveryInspection<'_, '_, W> {
    pub(crate) fn observation(&self) -> &RecoveryObservation {
        &self.owner.observation
    }
    pub(crate) fn prefix(&self) -> &RecoveryPrefixObservation {
        self.owner.prefix.observation()
    }
    pub(crate) fn effects(&self) -> &RecoveryEffects {
        &self.owner.observation.effects
    }
    pub(crate) fn allocator_image(&self) -> &[u8; PAGE_BYTES] {
        self.owner.maintenance_tail[HOME_A..HOME_A + PAGE_BYTES]
            .try_into()
            .expect("physical allocator")
    }
    /// The last initialized Status, preserving the first-INIT observation API.
    pub(crate) fn status_image(&self) -> Option<&[u8; PAGE_BYTES]> {
        self.status_image_at(self.owner.prefix.observation.last_status_page as u64)
    }
    /// Borrow an immutable recovered target; uninitialized addresses are absent.
    pub(crate) fn status_image_at(&self, page_no: u64) -> Option<&[u8; PAGE_BYTES]> {
        if !(1..TARGET_COUNT as u64).contains(&page_no)
            || self.owner.prefix.observation.status_mask & (1u64 << page_no) == 0
        {
            return None;
        }
        let at = page_no as usize * PAGE_BYTES;
        Some(
            self.owner.maintenance_tail[at..at + PAGE_BYTES]
                .try_into()
                .expect("physical Status"),
        )
    }
    /// An immutable recovered initial Free Undo root. This observation grants
    /// neither a root role nor live Undo/chunk/transaction authority.
    pub(crate) fn undo_image_at(&self, page_no: u64) -> Option<&[u8; PAGE_BYTES]> {
        if !(1..TARGET_COUNT as u64).contains(&page_no)
            || self.owner.prefix.observation.scope != RecoveryScope::MixedEffects
            || self.owner.prefix.observation.undo_mask & (1u64 << page_no) == 0
        {
            return None;
        }
        let at = page_no as usize * PAGE_BYTES;
        Some(
            self.owner.maintenance_tail[at..at + PAGE_BYTES]
                .try_into()
                .expect("physical initial Undo"),
        )
    }
    pub(crate) fn transaction_descriptor_at(&self, index: usize) -> Option<&[u8; 1536]> {
        self.owner
            .prefix
            .transaction_arena
            .as_deref()
            .and_then(|arena| transactions::descriptor_at(arena, index))
    }
    pub(crate) fn descriptor_bytes(&self) -> Option<&[u8; 1536]> {
        self.owner.prefix.descriptor_bytes()
    }
}
impl<W: WriterNamespace> RecoveryFailure<'_, '_, W> {
    pub(crate) fn error(&self) -> &RecoveryError {
        &self.error
    }
    pub(crate) fn observation(&self) -> &RecoveryObservation {
        &self.owner.observation
    }
    pub(crate) fn prefix(&self) -> &RecoveryPrefixObservation {
        self.owner.prefix.observation()
    }
    pub(crate) fn effects(&self) -> &RecoveryEffects {
        &self.owner.observation.effects
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "native_tests.rs"]
pub(in crate::storage::wal_io) mod native;
#[cfg(test)]
mod tests;
