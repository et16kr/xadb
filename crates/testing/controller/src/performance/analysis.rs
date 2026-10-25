use super::{CampaignKind, Order, Recipe, Side};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use test_protocol::performance::{DiagnosticsMode, Input, Operation, Output, PATH};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunSample {
    pub run: [u8; 16],
    pub output: Option<Output>,
    /// Duration from external Invoke to Observe, in the controller clock domain.
    pub external_ns: Option<u64>,
    pub error: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Ratio {
    pub numerator: u64,
    pub denominator: u64,
}
impl Ratio {
    fn new(numerator: u64, denominator: u64) -> Option<Self> {
        (denominator != 0).then_some(Self {
            numerator,
            denominator,
        })
    }
    fn cmp(self, other: Self) -> Ordering {
        (u128::from(self.numerator) * u128::from(other.denominator))
            .cmp(&(u128::from(other.numerator) * u128::from(self.denominator)))
    }
    fn threshold(self, numerator: u64, denominator: u64) -> Ordering {
        self.cmp(Self {
            numerator,
            denominator,
        })
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Comparison {
    pub validity: String,
    pub correctness: String,
    pub performance_effect: String,
    pub guardrails: String,
    pub cause_status: String,
    pub promotion: String,
    pub reasons: Vec<String>,
    pub ratios: Vec<Ratio>,
    pub interval: Option<[Ratio; 2]>,
    /// Exact average of fifth and sixth ordered ratios; no floating-point decision.
    pub median: Option<[u128; 2]>,
    pub nominal_interval_coverage: [u32; 2],
    pub worst_elapsed: Option<Ratio>,
    pub worst_cpu: Option<Ratio>,
    pub worst_external: Option<Ratio>,
    pub worst_sample_p99: Option<Ratio>,
    pub worst_process_peak_rss_delta: Option<i128>,
}
impl Default for Comparison {
    fn default() -> Self {
        Self {
            validity: "VALID".into(),
            correctness: "PASS".into(),
            performance_effect: "INCONCLUSIVE".into(),
            guardrails: "PASS".into(),
            cause_status: "UNRESOLVED".into(),
            promotion: "INCOMPLETE".into(),
            reasons: Vec::new(),
            ratios: Vec::new(),
            interval: None,
            median: None,
            nominal_interval_coverage: [1002, 1024],
            worst_elapsed: None,
            worst_cpu: None,
            worst_external: None,
            worst_sample_p99: None,
            worst_process_peak_rss_delta: None,
        }
    }
}

/// Scalar model of the published workload. No engine/model implementation is shared.
/// A mismatch rejects correctness; missing work is also invalid for performance.
pub fn check_output(
    input: &Input,
    side: &Side,
    output: &Output,
) -> std::result::Result<(), &'static str> {
    if !input.validate()
        || output.version != 1
        || output.input != *input
        || output.path != PATH
        || output.build != side.build
    {
        return Err("input/path/build identity mismatch");
    }
    if output.error.is_some()
        || !output.events_available
        || output.warmup.completed != input.warmup
        || output.measurement.completed != input.measure
        || output.read_values.len() != input.measure as usize
        || output.latency_ns.len() != (input.measure / input.latency_stride) as usize
        || output.final_values.len() != input.row_count as usize
    {
        return Err("failed or incomplete workload");
    }
    let mut values: Vec<_> = (0..input.row_count).map(i64::from).collect();
    let mut state = input.seed;
    for iteration in 0..u64::from(input.warmup) + u64::from(input.measure) {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let value = state as i64;
        // Each phase starts indexing at zero. Warmup is divisible by both row counts.
        let row = (iteration % u64::from(input.row_count)) as usize;
        values[row] = value;
        if iteration >= u64::from(input.warmup)
            && output.read_values[(iteration - u64::from(input.warmup)) as usize] != value
        {
            return Err("independent scalar read mismatch");
        }
    }
    if output.final_values != values {
        return Err("independent final row mismatch");
    }
    let old = input
        .pin_old
        .then(|| (0..input.row_count).map(i64::from).collect::<Vec<_>>());
    if output.old_values != old {
        return Err("independent old snapshot mismatch");
    }
    let before = &output.metrics_before;
    let after = &output.metrics_after;
    if after.operations.checked_sub(before.operations) != Some(u64::from(input.measure))
        || after.rows_read.checked_sub(before.rows_read) != Some(u64::from(input.measure))
        || after
            .row_references_copied
            .checked_sub(before.row_references_copied)
            .is_none()
        || [before.resident_bytes, after.resident_bytes]
            .iter()
            .any(|v| *v > 64 * 1024 * 1024)
        || before.snapshot_pins != u64::from(input.pin_old)
        || after.snapshot_pins != u64::from(input.pin_old)
    {
        return Err("basic counter delta or bounded gauge mismatch");
    }
    let before = &output.diagnostics_before;
    let after = &output.diagnostics_after;
    for snapshot in [before, after] {
        if snapshot.mode != input.mode
            || snapshot.logical_bytes > 64 * 1024
            || snapshot.event_capacity != input.capacity
        {
            return Err("diagnostics mode/budget mismatch");
        }
    }
    if input.mode == DiagnosticsMode::Off {
        if [before, after].iter().any(|s| {
            s.operations.is_some()
                || s.dropped_events.is_some()
                || s.saturated.is_some()
                || s.logical_bytes != 0
        }) || !output.events.is_empty()
        {
            return Err("off diagnostics must be uncollected");
        }
        return Ok(());
    }
    let prior = before
        .operations
        .as_ref()
        .ok_or("missing diagnostics counters")?;
    let current = after
        .operations
        .as_ref()
        .ok_or("missing diagnostics counters")?;
    if before.saturated != Some(false) || after.saturated != Some(false) {
        return Err("saturated or unknown diagnostics counters");
    }
    for index in 0..11 {
        let expected = if [3, 5, 8].contains(&index) {
            u64::from(input.measure)
        } else {
            0
        };
        if current[index].started.checked_sub(prior[index].started) != Some(expected)
            || current[index].completed.checked_sub(prior[index].completed) != Some(expected)
            || current[index].failed.checked_sub(prior[index].failed) != Some(0)
        {
            return Err("diagnostics independent operation counts mismatch");
        }
    }
    if input.mode == DiagnosticsMode::Always {
        if !output.events.is_empty()
            || before.dropped_events.is_some()
            || after.dropped_events.is_some()
        {
            return Err("always mode unexpectedly collected events");
        }
    } else {
        let dropped = after
            .dropped_events
            .zip(before.dropped_events)
            .and_then(|(a, b)| a.checked_sub(b))
            .ok_or("missing/regressed event drop counter")?;
        let attempted = u64::from(input.measure) * 3;
        let retained = u64::from(input.capacity).min(attempted);
        if output.events.len() as u64 != retained || dropped != attempted - retained {
            return Err("event/drop accounting mismatch");
        }
        let sequence_base = prior
            .iter()
            .try_fold(0u64, |total, counts| total.checked_add(counts.completed))
            .ok_or("event sequence base overflow")?;
        for (index, event) in output.events.iter().enumerate() {
            let expected_sequence = sequence_base
                .checked_add(index as u64 + 1)
                .ok_or("event sequence overflow")?;
            let expected_operation =
                [Operation::Replace, Operation::Snapshot, Operation::Read][index % 3];
            if event.error.is_some()
                || event.operation != expected_operation
                || event.sequence != expected_sequence
            {
                return Err("invalid detailed event sequence/outcome");
            }
        }
    }
    Ok(())
}
fn worst(target: &mut Option<Ratio>, value: Ratio) {
    if target.is_none_or(|old| value.cmp(old) == Ordering::Greater) {
        *target = Some(value);
    }
}
fn p99(output: &Output) -> u64 {
    let mut values = output.latency_ns.clone();
    values.sort_unstable();
    values[4055]
}
fn invalid(result: &mut Comparison, reason: impl Into<String>) {
    result.validity = "INVALID".into();
    result.reasons.push(reason.into());
}
fn guard(
    result: &mut Comparison,
    baseline: Option<u64>,
    candidate: Option<u64>,
    threshold: u64,
    kind: usize,
) {
    match baseline.zip(candidate).and_then(|(a, b)| Ratio::new(b, a)) {
        Some(ratio) => {
            if ratio.threshold(threshold, 100) == Ordering::Greater {
                result.guardrails = "FAIL".into();
            }
            worst(
                match kind {
                    0 => &mut result.worst_elapsed,
                    1 => &mut result.worst_cpu,
                    2 => &mut result.worst_external,
                    _ => &mut result.worst_sample_p99,
                },
                ratio,
            );
        }
        None => {
            if result.guardrails != "FAIL" {
                result.guardrails = "UNKNOWN".into();
            }
            result
                .reasons
                .push("missing guardrail observation or zero denominator".into());
        }
    }
}

/// Analyze exactly 20 bound runs. Failed, missing or reordered slots cannot disappear.
/// The comparison is local evidence; this API never promotes an optimization by itself.
pub fn analyze(recipe: &Recipe, samples: &[RunSample]) -> Comparison {
    let mut result = Comparison::default();
    if let Err(error) = recipe.validate() {
        invalid(&mut result, error.to_string());
    }
    if samples.len() != 20 {
        invalid(&mut result, "missing/extra campaign runs");
        result.correctness = "UNKNOWN".into();
    }
    let mut ab = Vec::new();
    let mut ba = Vec::new();
    for (index, pair) in recipe.pairs.iter().enumerate() {
        let Some(a) = samples.get(index * 2) else {
            continue;
        };
        let Some(b) = samples.get(index * 2 + 1) else {
            continue;
        };
        if a.run != pair.baseline_run || b.run != pair.candidate_run {
            invalid(&mut result, "run identity/order mismatch");
        }
        let (Some(baseline), Some(candidate)) = (&a.output, &b.output) else {
            invalid(&mut result, "failed/missing run output");
            if result.correctness != "FAIL" {
                result.correctness = "UNKNOWN".into();
            }
            continue;
        };
        let mut usable = true;
        for (sample, output, side, is_candidate) in [
            (a, baseline, &recipe.baseline, false),
            (b, candidate, &recipe.candidate, true),
        ] {
            if sample.error.is_some() {
                invalid(&mut result, "externally failed run");
                usable = false;
            }
            let input = recipe.input(is_candidate);
            // Durations are independently measured in their own clock domains.
            // Only containment is checked; no cross-process timestamps are subtracted.
            let phase_ns = u128::from(output.setup_ns)
                + u128::from(output.warmup.elapsed_ns)
                + u128::from(output.measurement.elapsed_ns);
            if output
                .latency_ns
                .iter()
                .any(|ns| *ns > output.measurement.elapsed_ns)
                || sample
                    .external_ns
                    .is_some_and(|ns| phase_ns > u128::from(ns))
            {
                invalid(&mut result, "impossible nested timing durations");
                usable = false;
            }
            if output.input != input
                || output.path != recipe.path
                || output.build != side.build
                || output.warmup.completed != input.warmup
                || output.measurement.completed != input.measure
                || output.error.is_some()
                || !output.events_available
                || output.read_values.len() != input.measure as usize
                || output.latency_ns.len() != 4096
                || output.measurement.elapsed_ns == 0
                || output.diagnostics_before.saturated == Some(true)
                || output.diagnostics_after.saturated == Some(true)
            {
                invalid(
                    &mut result,
                    "run identity/workload/timing/coverage mismatch",
                );
                usable = false;
            }
            if let Err(reason) = check_output(&input, side, output) {
                result.correctness = "FAIL".into();
                result.reasons.push(reason.into());
                usable = false;
            }
        }
        if !usable {
            continue;
        }
        let Some(ratio) = Ratio::new(
            candidate.measurement.elapsed_ns,
            baseline.measurement.elapsed_ns,
        ) else {
            invalid(&mut result, "zero primary denominator");
            continue;
        };
        result.ratios.push(ratio);
        match pair.order {
            Order::AB => ab.push(ratio),
            Order::BA => ba.push(ratio),
        }
        guard(
            &mut result,
            Some(baseline.measurement.elapsed_ns),
            Some(candidate.measurement.elapsed_ns),
            115,
            0,
        );
        guard(&mut result, baseline.cpu_ns, candidate.cpu_ns, 115, 1);
        guard(&mut result, a.external_ns, b.external_ns, 115, 2);
        guard(
            &mut result,
            Some(p99(baseline)),
            Some(p99(candidate)),
            125,
            3,
        );
        match baseline
            .process_peak_rss_bytes
            .zip(candidate.process_peak_rss_bytes)
        {
            Some((a, b)) => {
                let delta = i128::from(b) - i128::from(a);
                result.worst_process_peak_rss_delta = Some(
                    result
                        .worst_process_peak_rss_delta
                        .map_or(delta, |old| old.max(delta)),
                );
                if delta > 4 * 1024 * 1024 {
                    result.guardrails = "FAIL".into();
                }
            }
            None => {
                if result.guardrails != "FAIL" {
                    result.guardrails = "UNKNOWN".into();
                }
            }
        }
    }
    if result.ratios.len() != 10 || ab.len() != 5 || ba.len() != 5 {
        invalid(&mut result, "not all ten pairs supplied usable evidence");
        if result.guardrails == "PASS" {
            result.guardrails = "UNKNOWN".into();
        }
    } else {
        ab.sort_by(|a, b| a.cmp(*b));
        ba.sort_by(|a, b| a.cmp(*b));
        let left = u128::from(ab[2].numerator) * u128::from(ba[2].denominator);
        let right = u128::from(ba[2].numerator) * u128::from(ab[2].denominator);
        let denominator = u128::from(ab[2].denominator) * u128::from(ba[2].denominator);
        match left.abs_diff(right).checked_mul(10) {
            Some(difference) if difference <= denominator => {}
            Some(_) => invalid(&mut result, "AB/BA median drift exceeds 0.10"),
            None => invalid(&mut result, "drift arithmetic overflow"),
        }
        let mut ordered = result.ratios.clone();
        ordered.sort_by(|a, b| a.cmp(*b));
        result.interval = Some([ordered[1], ordered[8]]);
        let numerator = (u128::from(ordered[4].numerator) * u128::from(ordered[5].denominator))
            .checked_add(u128::from(ordered[5].numerator) * u128::from(ordered[4].denominator));
        let denominator = (u128::from(ordered[4].denominator) * u128::from(ordered[5].denominator))
            .checked_mul(2);
        if let Some(median) = numerator.zip(denominator) {
            result.median = Some([median.0, median.1]);
        } else {
            invalid(&mut result, "median arithmetic overflow");
        }
        if result.validity == "VALID" {
            result.performance_effect = if ordered[8].threshold(95, 100) == Ordering::Less {
                "IMPROVED"
            } else if ordered[1].threshold(105, 100) == Ordering::Greater {
                "REGRESSED"
            } else if ordered[1].threshold(95, 100) != Ordering::Less
                && ordered[8].threshold(105, 100) != Ordering::Greater
            {
                "NO_MATERIAL_CHANGE"
            } else {
                "INCONCLUSIVE"
            }
            .into();
        }
    }
    // This leaf has no independent cause experiment or full optimization regression receipt.
    // Even Confirmation is incomplete until those obligations are supplied elsewhere.
    if !matches!(
        recipe.kind,
        CampaignKind::InstrumentationAudit | CampaignKind::ReplayAudit
    ) && (result.correctness == "FAIL" || result.guardrails == "FAIL")
    {
        result.promotion = "REJECTED".into();
    }
    result
}
