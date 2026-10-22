//! Borrowed diagnostics for the closed recovery owner. None is an authority.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecoveryPhase {
    Preflight,
    Lease,
    Prefix,
    Inventory,
    BootstrapProvision,
    SourceStabilize,
    PendingValidate,
    PendingHome,
    Retirement,
    ReserveRestore,
    Redo,
    Flush,
    Verify,
    Snapshot,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LeaseEffect {
    NotAcquired,
    MayHaveChanged,
}
#[derive(Debug)]
pub(crate) enum RecoveryCause {
    Capacity,
    Unsupported,
    IdentityMismatch,
    LengthMismatch,
    PriorMismatch,
    Invariant,
    Fenced,
    ArithmeticOverflow,
    Namespace(NamespaceError),
    Io(xadb_foundation::Error),
    Control(control::ControlError),
    PageIo(crate::storage::page_io::PageIoError),
    Allocation(allocation_page::AllocationError),
    Doublewrite(dwb::DoublewriteError),
    Wal(crate::storage::wal_frame::WalError),
    Semantic(super::super::SemanticError),
    Prefix(super::super::RecoveryPrefixError),
}
#[derive(Debug)]
pub(crate) struct RecoveryError {
    pub(crate) phase: RecoveryPhase,
    pub(crate) cause: RecoveryCause,
    pub(crate) effect: Effect,
    pub(crate) lease_effect: LeaseEffect,
}
pub(crate) struct RecoveryAcquireError {
    pub(crate) error: RecoveryError,
    pub(crate) effects: RecoveryEffects,
}
impl std::fmt::Debug for RecoveryAcquireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}
impl RecoveryAcquireError {
    pub(crate) fn error(&self) -> &RecoveryError {
        &self.error
    }
    pub(crate) fn effects(&self) -> &RecoveryEffects {
        &self.effects
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CallKind {
    OpenRead,
    OpenWrite,
    Metadata,
    Read,
    Create,
    Write,
    FileSync,
    DirectorySync,
    Publish,
    Inventory,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RecoveryCall {
    pub(crate) phase: RecoveryPhase,
    pub(crate) kind: CallKind,
    pub(crate) name: &'static str,
    pub(crate) offset: u64,
    pub(crate) length: u64,
}
#[derive(Debug, Default)]
pub(crate) struct RecoveryEffects {
    pub(crate) calls_attempted: u64,
    pub(crate) calls_completed: u64,
    pub(crate) mutation_attempted: u64,
    pub(crate) mutation_completed: u64,
    pub(crate) last_call: Option<RecoveryCall>,
    pub(crate) slot_create_attempted: u8,
    pub(crate) slot_created: u8,
    pub(crate) slot_zero_started: u8,
    pub(crate) slot_chunks_done: [u8; 2],
    pub(crate) slot_sync_attempted: u8,
    pub(crate) slot_synced: u8,
    pub(crate) boot_directory_sync_attempted: bool,
    pub(crate) boot_directory_synced: bool,
    pub(crate) source_sync_attempted: bool,
    pub(crate) source_synced: bool,
    pub(crate) source_directory_synced: bool,
    pub(crate) home_write_attempted: u8,
    pub(crate) home_written: u8,
    pub(crate) home_sync_attempted: u8,
    pub(crate) home_synced: u8,
    pub(crate) dwb_publish_attempted: u64,
    pub(crate) dwb_publish_completed: u64,
}
#[derive(Debug, Default)]
pub(crate) struct BootObservation {
    pub(crate) initial_lengths: [Option<u64>; 2],
    pub(crate) initial_stamps: [Option<FileMetadata>; 2],
    pub(crate) provision_stamps: [Option<FileMetadata>; 2],
    pub(crate) slots_provisioned: bool,
    pub(crate) inventory_entries: usize,
    pub(crate) inventory_capacity: usize,
    pub(crate) inventory_backing_bytes: usize,
}
pub(crate) struct LedgerObservation {
    /// Sum at the last successful exact stamps/known rename. After an uncertain
    /// write or publish this is not a fresh inventory (nor a claimed lower
    /// bound): attempted/chunk and possible-selection observations describe the
    /// unresolved effects. Outstanding/peak credit stays conservative. Never
    /// spend this diagnostic copy.
    pub(crate) actual_owned: u64,
    pub(crate) outstanding: u64,
    pub(crate) owned_peak: u64,
    pub(crate) protected: u64,
    pub(crate) remaining: crate::storage::obligation::RemainingBudget,
    pub(crate) replay_topup: u64,
    pub(crate) slot_growth: u64,
    pub(crate) dwb_stage_actual: u64,
    pub(crate) dwb_stage_credit: u64,
    pub(crate) current_credit: u64,
    pub(crate) writer_stage_credit: u64,
    pub(crate) epoch_credit: u64,
    pub(crate) restored_once: bool,
}
impl Default for LedgerObservation {
    fn default() -> Self {
        Self {
            actual_owned: 0,
            outstanding: 0,
            owned_peak: 0,
            protected: 0,
            remaining: crate::storage::obligation::RemainingBudget::default(),
            replay_topup: 0,
            slot_growth: 0,
            dwb_stage_actual: 0,
            dwb_stage_credit: 0,
            current_credit: 8192,
            writer_stage_credit: 4096,
            epoch_credit: 0,
            restored_once: false,
        }
    }
}
#[derive(Debug, Default)]
pub(crate) struct PhysicalObservation {
    pub(crate) observed_mask: u8,
    pub(crate) initial_lsn: [Option<u64>; 2],
    pub(crate) final_lsn: [Option<u64>; 2],
    pub(crate) applied: [u64; 2],
    pub(crate) skipped: [u64; 2],
    pub(crate) dirty_mask: u8,
    pub(crate) flushed_mask: u8,
    pub(crate) pending_repaired: bool,
    pub(crate) pending_home_skipped: bool,
    pub(crate) pending_retired: bool,
    pub(crate) canonical_passes: u8,
}
pub(crate) struct RecoveryObservation {
    pub(crate) phase: RecoveryPhase,
    pub(crate) lease_effect: LeaseEffect,
    pub(crate) source_stabilized: bool,
    pub(crate) known: SelectedControl,
    pub(crate) possible: [Option<SelectedControl>; 2],
    pub(crate) batch: u64,
    pub(crate) state: dwb::ControlState,
    pub(crate) boot: BootObservation,
    pub(crate) ledger: LedgerObservation,
    pub(crate) physical: PhysicalObservation,
    pub(crate) effects: RecoveryEffects,
    pub(crate) fd_peak: u8,
    pub(crate) source_census: SourceCensus,
}
