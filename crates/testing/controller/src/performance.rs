//! K0 paired measurement orchestration and independent scalar/statistical analysis.
//! This leaf qualifies neither durability nor a production optimization.
mod analysis;
mod bundle;
pub mod gate;

pub use analysis::{Comparison, RunSample, analyze, check_output};
pub use bundle::{Options, ReplayOptions, Report, replay, run, verify};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use test_protocol::performance::{BuildIdentity, DiagnosticsMode, Input, PATH};

pub const RECIPE_BYTES: usize = 64 * 1024;
pub const POLICY: &str = "k0-005-paired-v1";
pub const SEMANTICS: &str = "k0-volatile-catalog-v1";
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CampaignKind {
    InstrumentationAudit,
    Exploration,
    Confirmation,
    ReplayAudit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Order {
    AB,
    BA,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pair {
    pub baseline_run: [u8; 16],
    pub candidate_run: [u8; 16],
    pub order: Order,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Side {
    pub worker_sha256: String,
    pub receipt_sha256: String,
    pub build: BuildIdentity,
    pub config_sha256: String,
    pub mode: DiagnosticsMode,
}
/// External build evidence; assertions from the worker alone do not prove provenance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildReceipt {
    pub schema_version: u32,
    pub worker_sha256: String,
    pub build: BuildIdentity,
    pub command: String,
    pub features: Vec<String>,
    pub config_sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exploration {
    pub candidate_sha256: String,
    /// Frozen baseline, configuration, fixture, semantics and analysis policy.
    pub contract_sha256: String,
    pub runs: Vec<[u8; 16]>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub exploration: Vec<Exploration>,
    pub confirmation_attempt: u32,
    pub previous_confirmation_runs: Vec<[u8; 16]>,
    pub original_recipe_sha256: Option<String>,
}
/// Frozen before execution. All 20 slots remain present even when a run fails.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub version: u32,
    pub campaign_id: [u8; 16],
    pub kind: CampaignKind,
    pub order_seed: u64,
    pub pairs: Vec<Pair>,
    /// Mode is Off here; each side supplies only its instrumentation mode.
    pub fixture: Input,
    pub path: String,
    pub semantics: String,
    pub policy: String,
    pub baseline: Side,
    pub candidate: Side,
    pub provenance: Provenance,
    pub hypothesis: String,
}

pub(crate) fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Deterministic Fisher-Yates permutation of five AB and five BA blocks.
pub fn paired_orders(mut seed: u64) -> [Order; 10] {
    let mut orders = [Order::AB; 10];
    orders[5..].fill(Order::BA);
    for index in (1..10).rev() {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        orders.swap(index, (seed % (index as u64 + 1)) as usize);
    }
    orders
}
impl Recipe {
    /// Strict profile validation; rejects changed thresholds and selection reuse.
    pub fn validate(&self) -> Result<()> {
        if self.version != 1
            || self.campaign_id == [0; 16]
            || self.pairs.len() != 10
            || !self.fixture.validate()
            || self.fixture.mode != DiagnosticsMode::Off
            || self.path != PATH
            || self.semantics != SEMANTICS
            || self.policy != POLICY
            || self.hypothesis.is_empty()
            || self.hypothesis.len() > 4096
        {
            return Err("unsupported performance recipe profile".into());
        }
        let mut runs = BTreeSet::new();
        for (pair, order) in self.pairs.iter().zip(paired_orders(self.order_seed)) {
            if pair.order != order {
                return Err("pair order differs from frozen seed".into());
            }
            for id in [pair.baseline_run, pair.candidate_run] {
                if id == [0; 16] || !runs.insert(id) {
                    return Err("duplicate/zero campaign run".into());
                }
            }
        }
        for side in [&self.baseline, &self.candidate] {
            if !digest(&side.worker_sha256)
                || !digest(&side.receipt_sha256)
                || !digest(&side.build.source_sha256)
                || !digest(&side.config_sha256)
                || side.build.profile != "release"
                || side.build.self_test
                || side.build.toolchain.is_empty()
                || side.build.target.is_empty()
            {
                return Err("invalid release build identity".into());
            }
        }
        if self.baseline.build.toolchain != self.candidate.build.toolchain
            || self.baseline.build.target != self.candidate.build.target
            || self.baseline.config_sha256 != self.candidate.config_sha256
        {
            return Err("baseline/candidate build configuration differs".into());
        }
        let provenance = &self.provenance;
        if provenance.exploration.len() > 16 {
            return Err("exploration budget exceeded".into());
        }
        let mut candidates = BTreeSet::new();
        let mut prior = BTreeSet::new();
        for exploration in &provenance.exploration {
            if !digest(&exploration.candidate_sha256)
                || !candidates.insert(&exploration.candidate_sha256)
                || exploration.contract_sha256 != self.comparison_contract_sha256()?
                || exploration.runs.is_empty()
                || exploration.runs.len() > 320
            {
                return Err("invalid exploration inventory".into());
            }
            for id in &exploration.runs {
                if *id == [0; 16] || !prior.insert(*id) {
                    return Err("duplicate exploration run".into());
                }
            }
        }
        for id in &provenance.previous_confirmation_runs {
            if *id == [0; 16] || !prior.insert(*id) {
                return Err("duplicate prior run".into());
            }
        }
        if !runs.is_disjoint(&prior) {
            return Err("reused exploration/confirmation run".into());
        }
        match self.kind {
            CampaignKind::InstrumentationAudit => {
                if self.baseline.worker_sha256 != self.candidate.worker_sha256
                    || self.baseline.build != self.candidate.build
                    || self.baseline.receipt_sha256 != self.candidate.receipt_sha256
                    || self.baseline.mode != DiagnosticsMode::Off
                    || self.candidate.mode == DiagnosticsMode::Off
                    || !provenance.exploration.is_empty()
                    || provenance.confirmation_attempt != 0
                    || !provenance.previous_confirmation_runs.is_empty()
                    || provenance.original_recipe_sha256.is_some()
                {
                    return Err("instrumentation audit changed implementation or lineage".into());
                }
            }
            CampaignKind::Exploration => {
                if provenance.confirmation_attempt != 0
                    || !provenance.previous_confirmation_runs.is_empty()
                    || provenance.original_recipe_sha256.is_some()
                {
                    return Err("exploration contains confirmation evidence".into());
                }
            }
            CampaignKind::Confirmation => {
                if self.baseline.mode != self.candidate.mode
                    || provenance.confirmation_attempt != 1
                    || !provenance.previous_confirmation_runs.is_empty()
                    || provenance.original_recipe_sha256.is_some()
                    || !candidates.contains(&self.candidate.worker_sha256)
                {
                    return Err("confirmation must use one frozen candidate and fresh runs".into());
                }
            }
            CampaignKind::ReplayAudit => {
                if !provenance
                    .original_recipe_sha256
                    .as_deref()
                    .is_some_and(digest)
                    || provenance.confirmation_attempt > 1
                    || provenance.previous_confirmation_runs.len() < 20
                {
                    return Err("replay lacks its original campaign".into());
                }
            }
        }
        Ok(())
    }
    /// Digest binds the baseline and comparison contract across candidate selection.
    pub fn comparison_contract_sha256(&self) -> Result<String> {
        Ok(crate::sha256(&serde_json::to_vec(&(
            &self.baseline,
            &self.fixture,
            &self.path,
            &self.semantics,
            &self.policy,
            self.candidate.mode,
        ))?))
    }
    pub(crate) fn input(&self, candidate: bool) -> Input {
        let mut input = self.fixture.clone();
        input.mode = if candidate {
            self.candidate.mode
        } else {
            self.baseline.mode
        };
        input
    }
    /// Parse with a byte limit before serde allocation; unknown/duplicate fields fail.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > RECIPE_BYTES {
            return Err("recipe exceeds 64 KiB".into());
        }
        let recipe: Self = serde_json::from_slice(bytes)?;
        recipe.validate()?;
        Ok(recipe)
    }
}

impl Recipe {
    /// Construct the first fixed 128-row, retained-snapshot instrumentation campaign.
    /// Caller persists its exact bytes and digest before passing it to `run`.
    pub fn instrumentation(
        campaign_id: [u8; 16],
        seed: u64,
        identity: Side,
        mode: DiagnosticsMode,
    ) -> Result<Self> {
        use sha2::{Digest, Sha256};
        let ids: Vec<[u8; 16]> = (0u8..20)
            .map(|slot| {
                let mut hash = Sha256::new();
                hash.update(campaign_id);
                hash.update([slot]);
                let hash = hash.finalize();
                let mut id = [0; 16];
                id.copy_from_slice(&hash[..16]);
                id
            })
            .collect();
        let mut baseline = identity.clone();
        baseline.mode = DiagnosticsMode::Off;
        let mut candidate = identity;
        candidate.mode = mode;
        let recipe = Self {
            version: 1,
            campaign_id,
            kind: CampaignKind::InstrumentationAudit,
            order_seed: seed,
            pairs: paired_orders(seed)
                .into_iter()
                .enumerate()
                .map(|(i, order)| Pair {
                    baseline_run: ids[i * 2],
                    candidate_run: ids[i * 2 + 1],
                    order,
                })
                .collect(),
            fixture: Input {
                version: 1,
                seed,
                row_count: 128,
                pin_old: true,
                mode: DiagnosticsMode::Off,
                capacity: 256,
                warmup: 4096,
                measure: 65536,
                latency_stride: 16,
            },
            path: PATH.into(),
            semantics: SEMANTICS.into(),
            policy: POLICY.into(),
            baseline,
            candidate,
            provenance: Provenance {
                exploration: Vec::new(),
                confirmation_attempt: 0,
                previous_confirmation_runs: Vec::new(),
                original_recipe_sha256: None,
            },
            hypothesis: "Measure the added instrumentation cost; no optimization or cause claim."
                .into(),
        };
        recipe.validate()?;
        Ok(recipe)
    }
}

/// Discoverable fixed capability; no mutable thresholds are accepted in recipes.
pub fn profile() -> serde_json::Value {
    serde_json::json!({"schema_version":1,"scope":"K0_VOLATILE_ONLY","policy":POLICY,
        "semantics":SEMANTICS,"path":PATH,"recipe_bytes":RECIPE_BYTES,"pairs":10,
        "orders":{"AB":5,"BA":5},"warmup":4096,"measure":65536,"latency_stride":16,
        "primary":"measurement.elapsed_ns","effect_interval_indices":[1,8],
        "nominal_coverage":[1002,1024],"interval_assumption":"independent identically distributed paired observations; drift screening does not prove this assumption", "sample_p99_index":4055,
        "operation_order":["Create","Drop","Insert","Replace","Delete","Snapshot","Lookup","Schema","Read","Scan","NextPage"],
        "metrics":[
            {"id":"diagnostics.operations","kind":"counter","unit":"API calls","scope":"store since creation","coverage":"Always/Detailed; Off is None","meaning":"started on entry, completed on Ok or Err, failed on Err; panic is not completed; saturation invalidates exact deltas","source":"crates/engine/src/diagnostics.rs"},
            {"id":"diagnostics.events","kind":"bounded event sequence","unit":"API completion events","scope":"Detailed collection only","coverage":"bounded queue; drops and unavailability explicit; sequence is not a timestamp","source":"crates/engine/src/diagnostics.rs"},
            {"id":"basic.operations/rows_read/row_references_copied","kind":"counter","unit":"operations/returned rows/copied references","scope":"store lifetime; before/after checked deltas","coverage":"all modes; not total allocator accounting","source":"crates/engine/src/metrics.rs"},
            {"id":"basic.resident_bytes/snapshot_pins","kind":"gauge","unit":"logical charged bytes/unique active pins","scope":"before and after independently; resident may decrease","coverage":"not process RSS","source":"crates/engine/src/metrics.rs"},
            {"id":"setup/warmup/measurement.elapsed_ns","kind":"duration","unit":"ns","scope":"worker Instant clock per phase; measured loop includes value recording and sampled clocks, excludes final correctness reads and serialization","coverage":"single-thread closed-loop replace/snapshot/read fixture","source":"crates/testing/engine-worker/src/performance.rs"},
            {"id":"latency_ns","kind":"sample","unit":"ns","scope":"worker Instant duration each sixteenth measured iteration including zero","coverage":"4096 raw samples; sorted index4055 is sampled p99, not population p99","source":"crates/testing/engine-worker/src/performance.rs"},
            {"id":"cpu_ns","kind":"duration","unit":"ns","scope":"calling thread CLOCK_THREAD_CPUTIME_ID delta over measurement","coverage":"None on missing/regressed/unsupported data; not elapsed minus wait","source":"crates/platform/src/performance.rs"},
            {"id":"process_peak_rss_bytes","kind":"high water gauge","unit":"bytes","scope":"process VmHWM at collection time","coverage":"whole process lifetime peak, not measurement-only memory; None unsupported","source":"crates/platform/src/performance.rs"},
            {"id":"external_ns","kind":"duration","unit":"ns","scope":"controller journal Invoke to Observe in one session Instant domain","coverage":"includes request transport, setup, warmup, measurement and serialization; never subtract worker timestamps","source":"crates/testing/controller/src/native.rs"}
        ],
        "guardrails":{"elapsed_ratio":[115,100],"cpu_ratio":[115,100],"external_ratio":[115,100],"sample_p99_ratio":[125,100],"process_peak_rss_delta_bytes":4194304},
        "promotion":"InstrumentationAudit and ReplayAudit are always INCOMPLETE"})
}
