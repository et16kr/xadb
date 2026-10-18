//! Typed XADBTest observations; no engine dependency or worker verdict.
pub mod engine;
pub mod frame;
pub mod history;
pub mod performance;

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_INPUT_BYTES: usize = 65_536;
pub const MAX_STEPS: usize = 128;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    CheckedAdd { left: i64, right: i64 },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub id: String,
    pub action: Operation,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Value {
    Int64(i64),
    Overflow,
}

/// Only operations cross into the worker; the oracle and expected values do not.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub protocol_version: u32,
    pub run_id: String,
    pub case_sha256: String,
    pub steps: Vec<Step>,
}

/// Observations are data. There is deliberately no worker verdict field.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub protocol_version: u32,
    pub run_id: String,
    pub case_sha256: String,
    pub sequence: usize,
    pub step_id: String,
    pub value: Value,
    pub witnesses: Vec<String>,
}
