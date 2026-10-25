use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use test_protocol::{MAX_INPUT_BYTES, MAX_STEPS, Step, Value};

pub const WITNESSES: &[&str] = &[
    "fixture.checked_add",
    "fixture.overflow",
    "foundation.checked_add",
    "foundation.overflow",
];

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub scenario_schema_version: u32,
    pub case_id: String,
    pub contracts: Vec<String>,
    pub steps: Vec<Step>,
    pub oracle: Oracle,
    pub required_witnesses: Vec<String>,
    pub limits: Limits,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Oracle {
    pub id: String,
    pub provenance: String,
    pub assertions: Vec<Assertion>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Assertion {
    pub step_id: String,
    pub expected: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub timeout_ms: u64,
    pub output_bytes_per_stream: usize,
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
}

pub fn parse(bytes: &[u8]) -> Result<Scenario, String> {
    if bytes.len() > MAX_INPUT_BYTES {
        return Err("XADBTEST-CASE-LIMIT: scenario exceeds 64 KiB".into());
    }
    let case: Scenario =
        serde_json::from_slice(bytes).map_err(|e| format!("XADBTEST-SCHEMA: {e}"))?;
    if case.scenario_schema_version != 1 {
        return Err("XADBTEST-SCHEMA-VERSION: expected scenario version 1".into());
    }
    if !identifier(&case.case_id)
        || case.contracts.is_empty()
        || case.contracts.iter().any(|s| !identifier(s))
    {
        return Err("XADBTEST-IDENTITY: case and contract identifiers required".into());
    }
    if case.steps.is_empty() || case.steps.len() > MAX_STEPS {
        return Err("XADBTEST-STEPS: require 1..=128 steps".into());
    }
    let ids: BTreeSet<_> = case.steps.iter().map(|s| s.id.as_str()).collect();
    if ids.len() != case.steps.len() || ids.iter().any(|id| !identifier(id)) {
        return Err("XADBTEST-STEPS: invalid or duplicate step identifier".into());
    }
    if case.oracle.id != "exact-values-v1" || case.oracle.provenance.trim().is_empty() {
        return Err(
            "XADBTEST-ORACLE: registered oracle and independent provenance required".into(),
        );
    }
    let asserted: BTreeSet<_> = case
        .oracle
        .assertions
        .iter()
        .map(|a| a.step_id.as_str())
        .collect();
    if asserted != ids || asserted.len() != case.oracle.assertions.len() {
        return Err("XADBTEST-ASSERTIONS: exactly one assertion per step required".into());
    }
    let witnesses: BTreeSet<_> = case.required_witnesses.iter().map(String::as_str).collect();
    if witnesses.is_empty()
        || witnesses.len() != case.required_witnesses.len()
        || witnesses.iter().any(|w| !WITNESSES.contains(w))
    {
        return Err("XADBTEST-WITNESSES: unique registered witnesses required".into());
    }
    if !(50..=10_000).contains(&case.limits.timeout_ms)
        || !(1024..=1_048_576).contains(&case.limits.output_bytes_per_stream)
    {
        return Err(
            "XADBTEST-LIMITS: timeout 50..=10000 ms; output 1024..=1048576 bytes per stream".into(),
        );
    }
    Ok(case)
}
