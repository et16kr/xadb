//! Fixed L0 observations. All elapsed durations are inclusive in one injected
//! clock domain; nested operations must not be summed into CPU or response time.
//! No labels, event buffers, collector locks, callbacks other than Clock, reset,
//! percentile estimates, or dynamic allocation are used while recording.
use super::error::{DurableError as Error, Result};
use std::{
    mem::size_of,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use xadb_foundation::runtime::{Clock, Quota, Reservation};

/// Number of fixed production observation sites. New sites change the catalog.
pub const DURABILITY_OPERATION_COUNT: usize = 21;

/// Stable finite operation labels; no database or row identifier becomes a label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum DurabilityOperation {
    /// Commit-frame encoding only; items are final mutations.
    WalEncode,
    /// One write_all_at call for a commit frame; items are commit frames.
    WalAppend,
    /// Direct commit/seal FileIo::sync_all calls; items are calls, excluding helper-internal syncs.
    WalSync,
    /// Inclusive rotation including seal, new header and WRITER selection.
    WalRotate,
    /// Inclusive new epoch installation, including metadata helper syncs.
    WalEpochInstall,
    /// Inclusive normal-open candidate selection and full validation.
    RecoveryInspect,
    /// Candidate logical replay discarded before construction; items are post-checkpoint batches.
    RecoveryValidation,
    /// Final private root reconstruction; items are post-checkpoint batches.
    RecoveryApply,
    /// Inclusive retained-file verification and durability barrier; items are retained files.
    RecoveryStabilize,
    /// Direct FileIo::sync_all calls in the stabilization barrier.
    RecoverySync,
    /// Inclusive immutable checkpoint preparation/write; items are pinned rows.
    CheckpointBuild,
    /// Checkpoint file write_all_at calls after exclusive file creation; items are calls.
    CheckpointWrite,
    /// Direct checkpoint FileIo::sync_all calls; items are calls.
    CheckpointSync,
    /// Inclusive CURRENT temp/write/sync/rename/directory-sync selection.
    CheckpointSelect,
    /// Inclusive exact-cut archive copy and CURRENT publication; items are immutable files.
    BackupCopy,
    /// Composite archive file create/write/sync/readback, not pure write latency.
    BackupFile,
    /// Wait to acquire the engine's namespace/state mutex, excluding hold time.
    NamespaceWait,
    /// Namespace/state mutex hold duration; includes work performed under it.
    NamespaceHold,
    /// Exact selected checkpoint/WRITER byte-closure validation for retention.
    RetentionInspect,
    /// Inclusive coordinated retirement and namespace cleanup.
    GarbageCollect,
    /// Direct namespace sync calls at explicitly instrumented sites only.
    DirectorySync,
}

const OPERATIONS: [DurabilityOperation; DURABILITY_OPERATION_COUNT] = [
    DurabilityOperation::WalEncode,
    DurabilityOperation::WalAppend,
    DurabilityOperation::WalSync,
    DurabilityOperation::WalRotate,
    DurabilityOperation::WalEpochInstall,
    DurabilityOperation::RecoveryInspect,
    DurabilityOperation::RecoveryValidation,
    DurabilityOperation::RecoveryApply,
    DurabilityOperation::RecoveryStabilize,
    DurabilityOperation::RecoverySync,
    DurabilityOperation::CheckpointBuild,
    DurabilityOperation::CheckpointWrite,
    DurabilityOperation::CheckpointSync,
    DurabilityOperation::CheckpointSelect,
    DurabilityOperation::BackupCopy,
    DurabilityOperation::BackupFile,
    DurabilityOperation::NamespaceWait,
    DurabilityOperation::NamespaceHold,
    DurabilityOperation::RetentionInspect,
    DurabilityOperation::GarbageCollect,
    DurabilityOperation::DirectorySync,
];

impl DurabilityOperation {
    /// Stable catalog ID. Revision 1 uses one engine-open lifetime without reset.
    pub const fn metric_id(self) -> &'static str {
        match self {
            Self::WalEncode => "dev2.wal.encode",
            Self::WalAppend => "dev2.wal.append",
            Self::WalSync => "dev2.wal.direct_sync",
            Self::WalRotate => "dev2.wal.rotate",
            Self::WalEpochInstall => "dev2.wal.epoch_install",
            Self::RecoveryInspect => "dev2.recovery.inspect",
            Self::RecoveryValidation => "dev2.recovery.validate",
            Self::RecoveryApply => "dev2.recovery.apply",
            Self::RecoveryStabilize => "dev2.recovery.stabilize",
            Self::RecoverySync => "dev2.recovery.direct_sync",
            Self::CheckpointBuild => "dev2.checkpoint.build",
            Self::CheckpointWrite => "dev2.checkpoint.write",
            Self::CheckpointSync => "dev2.checkpoint.direct_sync",
            Self::CheckpointSelect => "dev2.checkpoint.select",
            Self::BackupCopy => "dev2.backup.copy",
            Self::BackupFile => "dev2.backup.file",
            Self::NamespaceWait => "dev2.namespace.wait",
            Self::NamespaceHold => "dev2.namespace.hold",
            Self::RetentionInspect => "dev2.retention.inspect",
            Self::GarbageCollect => "dev2.retention.collect",
            Self::DirectorySync => "dev2.namespace.direct_sync",
        }
    }
    /// Code locator, to be qualified with the run's actual source/binary digest.
    pub const fn source_locator(self) -> &'static str {
        match self {
            Self::WalEncode
            | Self::WalAppend
            | Self::WalSync
            | Self::WalRotate
            | Self::WalEpochInstall => "crates/engine/src/durable/wal.rs",
            Self::RecoveryApply => {
                "crates/engine/src/durable/recovery.rs;crates/engine/src/database/maintenance.rs"
            }
            Self::RecoveryInspect
            | Self::RecoveryValidation
            | Self::RecoveryStabilize
            | Self::RecoverySync
            | Self::RetentionInspect => "crates/engine/src/durable/recovery.rs",
            Self::CheckpointBuild | Self::CheckpointWrite | Self::CheckpointSync => {
                "crates/engine/src/durable/checkpoint.rs"
            }
            Self::BackupCopy | Self::BackupFile => "crates/engine/src/durable/backup.rs",
            Self::CheckpointSelect => "crates/engine/src/database{,/maintenance}.rs",
            Self::GarbageCollect => "crates/engine/src/database/maintenance.rs",
            Self::NamespaceWait | Self::NamespaceHold => "crates/engine/src/database.rs",
            Self::DirectorySync => {
                "crates/engine/src/durable/{wal,recovery,checkpoint,backup}.rs;crates/engine/src/database/maintenance.rs"
            }
        }
    }
    fn can_partially_write(self) -> bool {
        matches!(
            self,
            Self::WalAppend | Self::CheckpointWrite | Self::BackupFile
        )
    }
    fn requested_bytes_defined(self) -> bool {
        matches!(
            self,
            Self::WalAppend | Self::CheckpointWrite | Self::BackupCopy | Self::BackupFile
        )
    }
    fn complete_bytes_defined(self) -> bool {
        matches!(
            self,
            Self::WalEncode
                | Self::WalAppend
                | Self::WalRotate
                | Self::WalEpochInstall
                | Self::CheckpointBuild
                | Self::CheckpointWrite
                | Self::BackupCopy
                | Self::BackupFile
        )
    }
    fn items_defined(self) -> bool {
        matches!(
            self,
            Self::WalEncode
                | Self::WalAppend
                | Self::WalSync
                | Self::RecoveryValidation
                | Self::RecoveryApply
                | Self::RecoveryStabilize
                | Self::CheckpointBuild
                | Self::CheckpointWrite
                | Self::CheckpointSync
                | Self::BackupCopy
        )
    }
}

/// Every completed timed scope, successful or failed, contributes one sample.
#[derive(Clone, Copy, Debug)]
pub struct DurabilityTiming {
    /// Observed completed scopes, not transaction throughput.
    pub samples: u64,
    /// Inclusive sum in nanoseconds. Never interpreted as CPU time.
    pub sum_ns: u64,
    /// Maximum observed inclusive duration in nanoseconds.
    pub max_ns: u64,
    /// Bucket 0 is 0..=1 ns; bucket k is 2^k..=2^(k+1)-1 ns;
    /// bucket 63 includes 2^63..=u64::MAX. No percentile is invented from it.
    pub log2_ns: [u64; 64],
}

/// Independent atomic snapshots; a concurrent read is not a coherent event cut.
#[derive(Clone, Copy, Debug)]
pub struct DurabilityOperationMetrics {
    /// Stable bounded catalog entry.
    pub operation: DurabilityOperation,
    /// Scope entries, including failures and still-running operations.
    pub attempts: u64,
    /// Scopes explicitly completed successfully.
    pub successes: u64,
    /// Failed/unwound scopes; observation never alters the product result.
    pub failures: u64,
    /// Requested wrapper bytes, not actual device traffic. None when this site
    /// does not measure byte requests, including recovery reads and sync calls.
    pub requested_bytes: Option<u64>,
    /// Bytes belonging to completely successful wrappers. A failed partial write
    /// contributes no bytes here; this is not a claim that it transferred zero.
    /// None for sites without byte coverage. Rotation/install cover WAL header
    /// and seal bytes only; selector/resume metadata bytes are excluded.
    pub confirmed_complete_bytes: Option<u64>,
    /// Entered work units, defined by the operation's documentation.
    pub items: Option<u64>,
    /// Failed write-capable wrappers whose physical partial byte count is unknown.
    pub unknown_partial_operations: u64,
    /// Completed scopes lacking a usable pair of clock observations.
    pub timing_missing: u64,
    /// None without a clock or after any invalid observation in this domain.
    pub elapsed: Option<DurabilityTiming>,
}

/// Live state supplied by the engine owner; not derived from unrelated counters.
#[derive(Clone, Copy, Debug, Default)]
pub struct DurabilityGauges {
    /// Whole WAL files currently retained, including required old segments.
    pub retained_wal_bytes: u64,
    /// Synced WAL position; publication may still be in progress.
    pub durable_position: u64,
    /// Selected exact checkpoint cut.
    pub checkpoint_position: u64,
    /// Reserved resident headroom owned by the system-control pool.
    pub control_reserved_bytes: u64,
    /// Actual control-pool leases; included within reserved headroom.
    pub control_used_bytes: u64,
    /// Oldest active system pin age in the observation clock domain. None without
    /// a usable clock, with observations disabled, or when no system pin is active.
    pub oldest_system_pin_age_ns: Option<u64>,
    /// Active system pins strictly older than 60 seconds. None without a usable
    /// clock or with observations disabled; an empty active set reports Some(0).
    pub system_pins_over_60_seconds: Option<u64>,
}

/// Bounded production observation snapshot. Detailed traces and CPU samples are
/// unsupported here; external PF bundles carry build identity and run intervals.
#[derive(Clone, Copy, Debug)]
pub struct DurabilityMetrics {
    /// Meaning revision of this operation catalog and histogram layout.
    pub semantic_revision: u32,
    /// None in off mode. Disabled counters must not be interpreted as zero work.
    pub operations: Option<[DurabilityOperationMetrics; DURABILITY_OPERATION_COUNT]>,
    /// Optional timing port was configured for this engine-open lifetime.
    pub clock_configured: bool,
    /// Clock panic, backward movement or exhausted timestamp invalidated timing.
    pub clock_invalid: bool,
    /// Some counter saturated, making exact subsequent deltas unavailable.
    pub counters_saturated: bool,
    /// Owner-provided instantaneous resource state.
    pub gauges: DurabilityGauges,
}

struct Counters {
    attempts: AtomicU64,
    successes: AtomicU64,
    failures: AtomicU64,
    requested: AtomicU64,
    complete: AtomicU64,
    items: AtomicU64,
    unknown_partial: AtomicU64,
    timing_missing: AtomicU64,
    samples: AtomicU64,
    sum: AtomicU64,
    max: AtomicU64,
    buckets: [AtomicU64; 64],
}
impl Counters {
    fn new() -> Self {
        Self {
            attempts: AtomicU64::new(0),
            successes: AtomicU64::new(0),
            failures: AtomicU64::new(0),
            requested: AtomicU64::new(0),
            complete: AtomicU64::new(0),
            items: AtomicU64::new(0),
            unknown_partial: AtomicU64::new(0),
            timing_missing: AtomicU64::new(0),
            samples: AtomicU64::new(0),
            sum: AtomicU64::new(0),
            max: AtomicU64::new(0),
            buckets: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }
}

pub(crate) struct Observations {
    enabled: bool,
    clock: Option<Arc<dyn Clock + Send + Sync>>,
    invalid: AtomicBool,
    saturated: AtomicBool,
    counters: [Counters; DURABILITY_OPERATION_COUNT],
    _charge: Reservation,
}
impl Observations {
    pub(crate) const fn allocation_bytes() -> usize {
        size_of::<Self>() + 2 * size_of::<usize>()
    }
    pub(crate) fn new(
        enabled: bool,
        clock: Option<Arc<dyn Clock + Send + Sync>>,
        quota: &Quota,
    ) -> Result<Arc<Self>> {
        let charge = quota
            .reserve(Self::allocation_bytes() as u64)
            .map_err(Error::foundation)?;
        Ok(Arc::new(Self {
            enabled,
            clock,
            invalid: AtomicBool::new(false),
            saturated: AtomicBool::new(false),
            counters: std::array::from_fn(|_| Counters::new()),
            _charge: charge,
        }))
    }
    /// Before sharing the observer with WAL/runtime owners, transfer its initial
    /// recovery-scratch charge into the engine's resident control pool.
    pub(crate) fn rehome(&mut self, quota: &Quota) -> Result<()> {
        let charge = quota
            .reserve(Self::allocation_bytes() as u64)
            .map_err(Error::foundation)?;
        self._charge = charge;
        Ok(())
    }
    fn add(&self, target: &AtomicU64, amount: u64) {
        let _ = target.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            Some(value.checked_add(amount).unwrap_or_else(|| {
                self.saturated.store(true, Ordering::Relaxed);
                u64::MAX
            }))
        });
    }
    fn now(&self) -> Option<u64> {
        if !self.enabled || self.invalid.load(Ordering::Relaxed) {
            return None;
        }
        self.clock.as_ref().and_then(|clock| {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| clock.now_nanos())) {
                Ok(value) if value != u64::MAX => Some(value),
                _ => {
                    self.invalid.store(true, Ordering::Relaxed);
                    None
                }
            }
        })
    }
    pub(crate) fn timestamp(&self) -> Option<u64> {
        self.now()
    }
    pub(crate) fn invalidate_clock(&self) {
        self.invalid.store(true, Ordering::Relaxed);
    }
    pub(crate) fn start(
        &self,
        operation: DurabilityOperation,
        requested: u64,
        items: u64,
    ) -> Span<'_> {
        if self.enabled {
            let counter = &self.counters[operation as usize];
            self.add(&counter.attempts, 1);
            self.add(&counter.requested, requested);
            self.add(&counter.items, items);
        }
        Span {
            owner: self,
            operation,
            start: self.now(),
            finished: false,
        }
    }
    pub(crate) fn snapshot(&self, gauges: DurabilityGauges) -> DurabilityMetrics {
        let invalid = self.invalid.load(Ordering::Relaxed);
        let timed = self.clock.is_some() && !invalid;
        let operations = self.enabled.then(|| {
            std::array::from_fn(|index| {
                let c = &self.counters[index];
                DurabilityOperationMetrics {
                    operation: OPERATIONS[index],
                    attempts: c.attempts.load(Ordering::Relaxed),
                    successes: c.successes.load(Ordering::Relaxed),
                    failures: c.failures.load(Ordering::Relaxed),
                    requested_bytes: OPERATIONS[index]
                        .requested_bytes_defined()
                        .then(|| c.requested.load(Ordering::Relaxed)),
                    confirmed_complete_bytes: OPERATIONS[index]
                        .complete_bytes_defined()
                        .then(|| c.complete.load(Ordering::Relaxed)),
                    items: OPERATIONS[index]
                        .items_defined()
                        .then(|| c.items.load(Ordering::Relaxed)),
                    unknown_partial_operations: c.unknown_partial.load(Ordering::Relaxed),
                    timing_missing: c.timing_missing.load(Ordering::Relaxed),
                    elapsed: timed.then(|| DurabilityTiming {
                        samples: c.samples.load(Ordering::Relaxed),
                        sum_ns: c.sum.load(Ordering::Relaxed),
                        max_ns: c.max.load(Ordering::Relaxed),
                        log2_ns: std::array::from_fn(|bucket| {
                            c.buckets[bucket].load(Ordering::Relaxed)
                        }),
                    }),
                }
            })
        });
        DurabilityMetrics {
            semantic_revision: 1,
            operations,
            clock_configured: self.clock.is_some(),
            clock_invalid: invalid,
            counters_saturated: self.saturated.load(Ordering::Relaxed),
            gauges,
        }
    }
}

pub(crate) struct Span<'a> {
    owner: &'a Observations,
    operation: DurabilityOperation,
    start: Option<u64>,
    finished: bool,
}
impl Span<'_> {
    pub(crate) fn finish(mut self, success: bool, complete_bytes: u64) {
        self.record(success, complete_bytes);
        self.finished = true;
    }
    fn record(&self, success: bool, complete_bytes: u64) {
        if !self.owner.enabled {
            return;
        }
        let c = &self.owner.counters[self.operation as usize];
        self.owner
            .add(if success { &c.successes } else { &c.failures }, 1);
        if success {
            self.owner.add(&c.complete, complete_bytes);
        } else if self.operation.can_partially_write() {
            self.owner.add(&c.unknown_partial, 1);
        }
        match (self.start, self.owner.now()) {
            (Some(start), Some(end)) if end >= start => {
                let elapsed = end - start;
                self.owner.add(&c.samples, 1);
                self.owner.add(&c.sum, elapsed);
                c.max.fetch_max(elapsed, Ordering::Relaxed);
                let bucket = if elapsed <= 1 {
                    0
                } else {
                    63 - elapsed.leading_zeros() as usize
                };
                self.owner.add(&c.buckets[bucket], 1);
            }
            (Some(_), Some(_)) => {
                self.owner.invalid.store(true, Ordering::Relaxed);
                self.owner.add(&c.timing_missing, 1);
            }
            _ => self.owner.add(&c.timing_missing, 1),
        }
    }
}
impl Drop for Span<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.record(false, 0);
        }
    }
}
