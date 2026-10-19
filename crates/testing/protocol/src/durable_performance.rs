//! One bounded durable-engine batch. Observations never contain an expected value
//! or comparison verdict. The primary path retains an old reader throughout all
//! warmup/measurement iterations, so it measures the retained-reader COW path.
use crate::{
    durable,
    frame::{Envelope, Error},
    performance::BuildIdentity,
};
use serde::{Deserialize, Serialize};
pub const VERSION: u32 = 1;
pub const PATH: &str = "dev2.retained_reader.sync_replace_readback.v1";
pub const ROWS: u32 = 128;
pub const WARMUP: u32 = 64;
pub const MEASURE: u32 = 512;
pub const OUTPUT_CAP: usize = 1024 * 1024;
pub const PHASE_COUNT: usize = 6;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub version: u32,
    pub seed: u64,
    pub row_count: u32,
    pub warmup: u32,
    pub measure: u32,
    pub observations: bool,
}
impl Input {
    pub fn validate(&self) -> bool {
        self.version == VERSION
            && self.row_count == ROWS
            && self.warmup == WARMUP
            && self.measure == MEASURE
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Setup,
    Warmup,
    Measurement,
    Scan,
    Checkpoint,
    GarbageCollect,
    Backup,
    Restore,
    Reopen,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "domain", rename_all = "snake_case", deny_unknown_fields)]
pub enum Failure {
    Product {
        stage: Stage,
        ordinal: Option<u32>,
        error: durable::Failure,
    },
    Harness {
        stage: Stage,
        ordinal: Option<u32>,
        detail: String,
    },
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Effect {
    pub attempts: u64,
    pub successes: u64,
    pub failures: u64,
    pub requested_bytes: Option<u64>,
    pub confirmed_bytes: Option<u64>,
    pub unknown_partial_calls: u64,
}
/// Adapter observations of actual forwarded OS operations, common to both modes.
/// Err from read/write leaves unknown partial bytes; confirmed bytes count only
/// returned Ok(n). These counters are not a power-loss durability proof.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Effects {
    pub create: Effect,
    pub open: Effect,
    pub read: Effect,
    pub write: Effect,
    pub file_sync: Effect,
    pub resize: Effect,
    pub rename: Effect,
    pub directory_sync: Effect,
    pub remove: Effect,
    pub saturated: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metrics {
    pub identity: Identity,
    pub core: durable::Metrics,
    pub durability: durable::DurabilityMetrics,
    pub effects: Effects,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub database: [u8; 16],
    pub timeline: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Directories {
    pub source: String,
    pub backup: String,
    pub restored: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Setup {
    pub elapsed_ns: u64,
    pub identity: Option<Identity>,
    pub table: Option<durable::TableIdentity>,
    pub ddl: Option<durable::CommitIdentity>,
    pub insert_begin: Option<durable::TransactionIdentity>,
    pub insert_commit: Option<durable::CommitIdentity>,
    pub rows: Vec<durable::RowIdentity>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Iteration {
    pub ordinal: u32,
    pub begin: Option<durable::TransactionIdentity>,
    pub commit: Option<durable::CommitIdentity>,
    pub read_value: Option<i64>,
    pub read_commit: Option<durable::CommitIdentity>,
    /// RW Begin through fresh RO read+commit, excluding generation/output append.
    pub elapsed_ns: u64,
    /// Exactly the RW commit call, including a failed returned call.
    pub commit_ns: Option<u64>,
    /// Fresh RO Begin through read+RO commit. Do not add overlapping durations.
    pub readback_ns: Option<u64>,
    pub completed: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Batch {
    pub elapsed_ns: u64,
    pub completed: u32,
    pub iterations: Vec<Iteration>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Maintenance {
    pub stage: Stage,
    pub elapsed_ns: u64,
    pub completed: bool,
    pub identity: Option<Identity>,
    pub position: Option<u64>,
    pub generation: Option<u64>,
    pub files: Option<u64>,
    pub bytes: Option<u64>,
    pub values: Vec<i64>,
    pub metrics_before: Option<Metrics>,
    pub metrics_after: Option<Metrics>,
    pub effects_before: Effects,
    pub effects_after: Effects,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Output {
    pub version: u32,
    pub path: String,
    pub input: Input,
    pub build: BuildIdentity,
    pub directories: Directories,
    pub setup: Setup,
    pub warmup: Batch,
    pub measurement: Batch,
    pub metrics_before: Option<Metrics>,
    pub metrics_after: Option<Metrics>,
    pub old_values: Vec<i64>,
    pub old_commit: Option<durable::CommitIdentity>,
    pub maintenance: Vec<Maintenance>,
    pub error: Option<Failure>,
    pub actual_affinity: Option<String>,
    pub cpu_ns: Option<u64>,
    pub process_peak_rss_bytes: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u32,
    pub run: [u8; 16],
    pub request: u64,
    pub actor: u8,
    pub input: Input,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub version: u32,
    pub run: [u8; 16],
    pub request: u64,
    pub actor: u8,
    pub output: Output,
}
fn identity(version: u32, run: [u8; 16], request: u64, actor: u8) -> Result<(), Error> {
    if version != VERSION || run == [0; 16] || request != 1 || actor != 0 {
        Err(Error::Invalid("durable performance identity"))
    } else {
        Ok(())
    }
}
impl Envelope for Request {
    fn validate(&self) -> Result<(), Error> {
        identity(self.version, self.run, self.request, self.actor)?;
        if !self.input.validate() {
            return Err(Error::Invalid("durable performance fixed input"));
        }
        Ok(())
    }
}
fn validate_metrics(metrics: &Metrics) -> Result<(), Error> {
    durable::Response {
        version: durable::VERSION,
        run: [1; 16],
        request: 1,
        actor: 0,
        passed_gate: None,
        observation: durable::Observation::DurabilityMetrics {
            counters: metrics.durability.clone(),
        },
    }
    .validate()
}
impl Envelope for Response {
    fn validate(&self) -> Result<(), Error> {
        identity(self.version, self.run, self.request, self.actor)?;
        let out = &self.output;
        if out.version != VERSION
            || out.path != PATH
            || !out.input.validate()
            || out.setup.rows.len() > ROWS as usize
            || out.old_values.len() > ROWS as usize
            || out.maintenance.len() > PHASE_COUNT
            || out.directories.source != "source"
            || out.directories.backup != "backup"
            || out.directories.restored != "restored"
            || out.build.source_sha256.len() > 128
            || out.build.toolchain.len() > 256
            || out.build.target.len() > 128
            || out.build.profile.len() > 32
            || out
                .actual_affinity
                .as_ref()
                .is_some_and(|affinity| affinity.len() > 1024)
        {
            return Err(Error::Invalid("durable performance output bound"));
        }
        for (batch, maximum) in [(&out.warmup, WARMUP), (&out.measurement, MEASURE)] {
            if batch.completed > maximum
                || batch.iterations.len() > maximum as usize
                || batch.completed as usize
                    != batch
                        .iterations
                        .iter()
                        .filter(|iteration| iteration.completed)
                        .count()
                || batch
                    .iterations
                    .iter()
                    .any(|iteration| iteration.ordinal >= WARMUP + MEASURE)
            {
                return Err(Error::Invalid("durable performance batch bound"));
            }
        }
        for metrics in [&out.metrics_before, &out.metrics_after]
            .into_iter()
            .flatten()
        {
            validate_metrics(metrics)?;
        }
        for phase in &out.maintenance {
            if phase.values.len() > ROWS as usize
                || matches!(
                    phase.stage,
                    Stage::Setup | Stage::Warmup | Stage::Measurement
                )
            {
                return Err(Error::Invalid("durable performance maintenance bound"));
            }
            for metrics in [&phase.metrics_before, &phase.metrics_after]
                .into_iter()
                .flatten()
            {
                validate_metrics(metrics)?;
            }
        }
        if let Some(error) = &out.error {
            match error {
                Failure::Harness { detail, .. } if detail.len() > 256 => {
                    return Err(Error::Invalid("durable performance harness error bound"));
                }
                Failure::Product { error, .. } => durable::Response {
                    version: durable::VERSION,
                    run: [1; 16],
                    request: 1,
                    actor: 0,
                    passed_gate: None,
                    observation: durable::Observation::Error {
                        failure: error.clone(),
                    },
                }
                .validate()?,
                _ => (),
            }
        }
        Ok(())
    }
}
