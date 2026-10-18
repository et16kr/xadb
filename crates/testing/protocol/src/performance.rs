//! Bounded K0 measurement observations. The controller owns all verdicts.
use crate::engine::Metrics;
use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 1;
pub const PATH: &str = "k0.replace_snapshot_read.v1";
pub const WARMUP: u32 = 4096;
pub const MEASURE: u32 = 65536;
pub const LATENCY_STRIDE: u32 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticsMode {
    Off,
    Always,
    Detailed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub version: u32,
    pub seed: u64,
    pub row_count: u32,
    pub pin_old: bool,
    pub mode: DiagnosticsMode,
    pub capacity: u32,
    pub warmup: u32,
    pub measure: u32,
    pub latency_stride: u32,
}
impl Input {
    /// Exact profile bounds, checked before any fixture or output allocation.
    pub fn validate(&self) -> bool {
        self.version == VERSION
            && matches!(self.row_count, 1 | 128)
            && self.capacity <= 1024
            && self.warmup == WARMUP
            && self.measure == MEASURE
            && self.latency_stride == LATENCY_STRIDE
    }
}

/// The actual compilation identity, independently bound to the retained binary
/// by the build receipt. A worker's declaration alone is not that receipt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildIdentity {
    pub source_sha256: String,
    pub toolchain: String,
    pub target: String,
    pub profile: String,
    pub self_test: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Create,
    Drop,
    Insert,
    Replace,
    Delete,
    Snapshot,
    Lookup,
    Schema,
    Read,
    Scan,
    NextPage,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationCounts {
    pub started: u64,
    /// All ordinary returns, including failed returns; excludes panic.
    pub completed: u64,
    pub failed: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticsSnapshot {
    pub mode: DiagnosticsMode,
    pub event_capacity: u32,
    pub logical_bytes: u64,
    /// Fixed operation order above. None means uncollected, never zero.
    pub operations: Option<[OperationCounts; 11]>,
    pub dropped_events: Option<u64>,
    pub saturated: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub sequence: u64,
    pub operation: Operation,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Phase {
    pub elapsed_ns: u64,
    pub completed: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Output {
    pub version: u32,
    pub input: Input,
    pub path: String,
    pub build: BuildIdentity,
    pub setup_ns: u64,
    pub warmup: Phase,
    pub measurement: Phase,
    /// Product/collection failure, never silently omitted from comparison.
    pub error: Option<String>,
    pub read_values: Vec<i64>,
    pub final_values: Vec<i64>,
    pub old_values: Option<Vec<i64>>,
    /// Every sixteenth measured iteration, including zero; 4096 samples.
    pub latency_ns: Vec<u64>,
    pub metrics_before: Metrics,
    pub metrics_after: Metrics,
    pub diagnostics_before: DiagnosticsSnapshot,
    pub diagnostics_after: DiagnosticsSnapshot,
    pub events: Vec<Event>,
    pub events_available: bool,
    pub cpu_ns: Option<u64>,
    /// Process high-water mark at sampling time, not a measurement-only peak.
    pub process_peak_rss_bytes: Option<u64>,
}
