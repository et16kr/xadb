//! Native durable-engine observations. Gate witnesses describe actor scheduling,
//! never a WAL fault cut or an independent durability verdict. K0 wire is unchanged.
//! Table/row aliases are global. Transaction/cursor aliases belong to the request
//! actor; each such alias is consumed once for that actor, including failed opens
//! and across database close/reopen. Every alias is 1..=1024; the run also has a
//! cumulative 1024-request bound. Insert results cannot be referenced by another
//! mutation until the statement has returned their actual RowUID capabilities.
pub use crate::engine::{
    Cell, Column, DataType, MAX_PENDING, MAX_REQUESTS, Row, RowIdentity, TableIdentity, TableSpec,
};
use crate::frame::{Envelope, Error};
use serde::{Deserialize, Serialize};
pub const VERSION: u32 = 1;
pub const MAX_DIRECTORIES: u8 = 8;
pub const MAX_GATES: u64 = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    ReadOnly,
    ReadWrite,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mutation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Mutation {
    Insert {
        table: u64,
        row: u64,
        values: Vec<Cell>,
    },
    Replace {
        table: u64,
        row: u64,
        values: Vec<Cell>,
    },
    Delete {
        table: u64,
        row: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    ConfigureFault {
        token: [u8; 16],
        plan: crate::fault::Plan,
    },
    ConfigureEngine {
        observations: bool,
        clock: bool,
    },
    CreateDatabase {
        directory: u8,
    },
    OpenDatabase {
        directory: u8,
    },
    CloseDatabase {},
    RestoreDatabase {
        source: u8,
        target: u8,
    },
    ConfigureGate {
        gate: u64,
        actors: Vec<u8>,
    },
    CreateTable {
        table: u64,
        spec: TableSpec,
    },
    DropTable {
        table: u64,
    },
    Begin {
        transaction: u64,
        mode: Mode,
    },
    Lookup {
        transaction: u64,
        table: u64,
        name: String,
    },
    /// Bind only an exact identity already returned by this worker's scan/read.
    BindRow {
        row: u64,
        identity: RowIdentity,
    },
    Statement {
        transaction: u64,
        mutations: Vec<Mutation>,
    },
    Read {
        transaction: u64,
        table: u64,
        row: u64,
    },
    OpenScan {
        transaction: u64,
        table: u64,
        cursor: u64,
    },
    NextPage {
        cursor: u64,
        limit: u32,
    },
    CloseScan {
        cursor: u64,
    },
    Commit {
        transaction: u64,
    },
    Rollback {
        transaction: u64,
    },
    Cancel {
        transaction: u64,
    },
    Checkpoint {},
    CollectGarbage {},
    Backup {
        directory: u8,
    },
    Metrics {},
    DurabilityMetrics {},
}
impl Action {
    pub fn exclusive(&self) -> bool {
        matches!(
            self,
            Self::ConfigureFault { .. }
                | Self::ConfigureEngine { .. }
                | Self::CreateDatabase { .. }
                | Self::OpenDatabase { .. }
                | Self::CloseDatabase { .. }
                | Self::RestoreDatabase { .. }
                | Self::ConfigureGate { .. }
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    /// One-shot actor rendezvous after dispatch, before the engine operation.
    pub gate: Option<u64>,
    pub action: Action,
}
impl From<Action> for Command {
    fn from(action: Action) -> Self {
        Self { gate: None, action }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransactionIdentity {
    pub database: [u8; 16],
    pub timeline: u64,
    pub writer_epoch: u32,
    pub local: u64,
}
/// Exact durable acknowledgement returned by the same DDL publication operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommitIdentity {
    pub transaction: TransactionIdentity,
    pub position: u64,
    pub wrote: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Active,
    Aborted,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Certainty {
    Known,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Retry {
    Never,
    Transaction,
    AfterRecovery,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    NotAttempted,
    Aborted,
    Unknown,
    Committed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "domain", rename_all = "snake_case", deny_unknown_fields)]
pub enum Failure {
    Transaction {
        code: String,
        effect: Effect,
        certainty: Certainty,
        retry: Retry,
        transaction: Option<TransactionIdentity>,
    },
    Storage {
        code: String,
        outcome: Outcome,
        fenced: bool,
        transaction: Option<TransactionIdentity>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssuedRow {
    pub alias: u64,
    pub identity: RowIdentity,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metrics {
    pub operations: u64,
    pub rows_read: u64,
    pub row_references_copied: u64,
    pub resident_bytes: u64,
    pub snapshot_pins: u64,
    pub private_bytes: u64,
    pub active_transactions: u64,
    pub oldest_user_pin_age_ns: Option<u64>,
    pub user_pins_over_sixty_seconds: Option<u64>,
    pub coordinator_acquisitions: u64,
    pub coordinator_wait_ns: Option<u64>,
    pub coordinator_hold_ns: Option<u64>,
    pub coordinator_clock_invalid: bool,
    pub counters_saturated: bool,
    pub serialization_conflicts: u64,
    pub statement_rollbacks: u64,
    pub commit_preparation_failures: u64,
}

/// Inclusive completed scopes; histogram bucket layout is semantic revision 1.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DurabilityTiming {
    pub samples: u64,
    pub sum_ns: u64,
    pub max_ns: u64,
    pub log2_ns: Vec<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DurabilityOperationMetrics {
    pub metric_id: String,
    pub attempts: u64,
    pub successes: u64,
    pub failures: u64,
    pub requested_bytes: Option<u64>,
    pub confirmed_complete_bytes: Option<u64>,
    pub items: Option<u64>,
    pub unknown_partial_operations: u64,
    pub timing_missing: u64,
    pub elapsed: Option<DurabilityTiming>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DurabilityGauges {
    pub retained_wal_bytes: u64,
    pub durable_position: u64,
    pub checkpoint_position: u64,
    pub control_reserved_bytes: u64,
    pub control_used_bytes: u64,
    pub oldest_system_pin_age_ns: Option<u64>,
    pub system_pins_over_60_seconds: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DurabilityMetrics {
    pub semantic_revision: u32,
    pub operations: Option<Vec<DurabilityOperationMetrics>>,
    pub clock_configured: bool,
    pub clock_invalid: bool,
    pub counters_saturated: bool,
    pub gauges: DurabilityGauges,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub enum Observation {
    Ready {
        database: [u8; 16],
        timeline: u64,
    },
    Done {},
    Table {
        identity: TableIdentity,
        spec: TableSpec,
        /// Some only for a successful CreateTable; lookup has no commit.
        commit: Option<CommitIdentity>,
    },
    Begun {
        identity: TransactionIdentity,
    },
    Statement {
        inserted: Vec<IssuedRow>,
        affected_rows: u64,
    },
    Row {
        value: Option<Row>,
    },
    Page {
        rows: Vec<Row>,
        eof: bool,
        canonical_bytes: u32,
    },
    Committed {
        transaction: TransactionIdentity,
        position: u64,
        wrote: bool,
    },
    Checkpoint {
        generation: u64,
        position: u64,
    },
    GarbageCollected {
        removed: u64,
    },
    Backup {
        generation: u64,
        position: u64,
    },
    Metrics {
        counters: Metrics,
    },
    DurabilityMetrics {
        counters: DurabilityMetrics,
    },
    Error {
        failure: Failure,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u32,
    pub run: [u8; 16],
    pub request: u64,
    pub actor: u8,
    pub command: Command,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub version: u32,
    pub run: [u8; 16],
    pub request: u64,
    pub actor: u8,
    /// Present only after this actor actually passed its configured rendezvous.
    pub passed_gate: Option<u64>,
    pub observation: Observation,
}

/// Closed catalog; meanings belong to source-qualified production observations.
pub const DURABILITY_METRIC_IDS: [&str; 21] = [
    "dev2.wal.encode",
    "dev2.wal.append",
    "dev2.wal.direct_sync",
    "dev2.wal.rotate",
    "dev2.wal.epoch_install",
    "dev2.recovery.inspect",
    "dev2.recovery.validate",
    "dev2.recovery.apply",
    "dev2.recovery.stabilize",
    "dev2.recovery.direct_sync",
    "dev2.checkpoint.build",
    "dev2.checkpoint.write",
    "dev2.checkpoint.direct_sync",
    "dev2.checkpoint.select",
    "dev2.backup.copy",
    "dev2.backup.file",
    "dev2.namespace.wait",
    "dev2.namespace.hold",
    "dev2.retention.inspect",
    "dev2.retention.collect",
    "dev2.namespace.direct_sync",
];

fn identity(version: u32, run: [u8; 16], request: u64, actor: u8) -> Result<(), Error> {
    if version != VERSION
        || run == [0; 16]
        || !(1..=MAX_REQUESTS).contains(&request)
        || usize::from(actor) >= MAX_PENDING
    {
        return Err(Error::Invalid("durable envelope identity"));
    }
    Ok(())
}
fn alias(value: u64) -> Result<(), Error> {
    if !(1..=MAX_REQUESTS).contains(&value) {
        return Err(Error::Invalid("durable alias bound"));
    }
    Ok(())
}
impl Envelope for Request {
    fn validate(&self) -> Result<(), Error> {
        identity(self.version, self.run, self.request, self.actor)?;
        if let Some(gate) = self.command.gate {
            if !(1..=MAX_GATES).contains(&gate) || self.command.action.exclusive() {
                return Err(Error::Invalid("invalid or exclusive rendezvous request"));
            }
        }
        match &self.command.action {
            Action::ConfigureFault { token, plan } => {
                if *token == [0; 16] {
                    return Err(Error::Invalid("zero fault control token"));
                }
                plan.validate()?;
            }
            Action::ConfigureGate { gate, actors } => {
                if !(1..=MAX_GATES).contains(gate)
                    || !(2..=MAX_PENDING).contains(&actors.len())
                    || actors
                        .iter()
                        .any(|actor| usize::from(*actor) >= MAX_PENDING)
                    || actors
                        .iter()
                        .enumerate()
                        .any(|(index, actor)| actors[..index].contains(actor))
                {
                    return Err(Error::Invalid("durable gate participants/id"));
                }
            }
            Action::CreateDatabase { directory }
            | Action::OpenDatabase { directory }
            | Action::Backup { directory } => {
                if *directory >= MAX_DIRECTORIES {
                    return Err(Error::Invalid("durable directory slot"));
                }
            }
            Action::RestoreDatabase { source, target } => {
                if *source >= MAX_DIRECTORIES || *target >= MAX_DIRECTORIES || source == target {
                    return Err(Error::Invalid("restore directory slots"));
                }
            }
            Action::Statement {
                transaction,
                mutations,
            } => {
                alias(*transaction)?;
                if mutations.len() > 256 {
                    return Err(Error::Invalid("durable statement count"));
                }
                for mutation in mutations {
                    let (table, row) = match mutation {
                        Mutation::Insert { table, row, .. }
                        | Mutation::Replace { table, row, .. }
                        | Mutation::Delete { table, row } => (*table, *row),
                    };
                    alias(table)?;
                    alias(row)?;
                }
            }
            Action::Begin { transaction, .. }
            | Action::Commit { transaction }
            | Action::Rollback { transaction }
            | Action::Cancel { transaction } => alias(*transaction)?,
            Action::Lookup {
                transaction, table, ..
            } => {
                alias(*transaction)?;
                alias(*table)?;
            }
            Action::CreateTable { table, .. } | Action::DropTable { table } => alias(*table)?,
            Action::BindRow { row, .. } => alias(*row)?,
            Action::Read {
                transaction,
                table,
                row,
            } => {
                alias(*transaction)?;
                alias(*table)?;
                alias(*row)?;
            }
            Action::OpenScan {
                transaction,
                table,
                cursor,
            } => {
                alias(*transaction)?;
                alias(*table)?;
                alias(*cursor)?;
            }
            Action::NextPage { cursor, limit } => {
                alias(*cursor)?;
                if !(1..=1024).contains(limit) {
                    return Err(Error::Invalid("durable page limit"));
                }
            }
            Action::CloseScan { cursor } => alias(*cursor)?,
            _ => (),
        }
        Ok(())
    }
}
impl Envelope for Response {
    fn validate(&self) -> Result<(), Error> {
        identity(self.version, self.run, self.request, self.actor)?;
        if self
            .passed_gate
            .is_some_and(|gate| !(1..=MAX_GATES).contains(&gate))
        {
            return Err(Error::Invalid("durable response gate"));
        }
        if let Observation::DurabilityMetrics { counters } = &self.observation {
            if counters.semantic_revision != 1 {
                return Err(Error::Invalid("durability metric revision"));
            }
            if let Some(operations) = &counters.operations {
                if operations.len() != 21
                    || operations.iter().enumerate().any(|(index, metric)| {
                        !DURABILITY_METRIC_IDS.contains(&metric.metric_id.as_str())
                            || operations[..index]
                                .iter()
                                .any(|previous| previous.metric_id == metric.metric_id)
                            || metric
                                .elapsed
                                .as_ref()
                                .is_some_and(|timing| timing.log2_ns.len() != 64)
                    })
                {
                    return Err(Error::Invalid("durability metric catalog/histogram"));
                }
            }
        }
        if let Observation::Error { failure } = &self.observation {
            let known = match failure {
                Failure::Transaction { code, .. } => matches!(
                    code.as_str(),
                    "XADB-ENGINE-0001"
                        | "XADB-ENGINE-0002"
                        | "XADB-ENGINE-0003"
                        | "XADB-ENGINE-0004"
                        | "XADB-ENGINE-0005"
                        | "XADB-ENGINE-0006"
                        | "XADB-ENGINE-0007"
                        | "XADB-ENGINE-0008"
                        | "XADB-ENGINE-0009"
                        | "ReadOnly"
                        | "SerializationConflict"
                        | "Cancelled"
                        | "Poisoned"
                        | "Fenced"
                        | "Closed"
                ),
                Failure::Storage { code, .. } => matches!(
                    code.as_str(),
                    "InvalidInput"
                        | "Corruption"
                        | "UnsupportedFormat"
                        | "LimitExceeded"
                        | "QuotaExceeded"
                        | "Io"
                        | "Busy"
                        | "Fenced"
                        | "IncompleteCreate"
                        | "Transaction"
                ),
            };
            if !known {
                return Err(Error::Invalid("unknown durable product error"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(action: Action) -> Request {
        Request {
            version: VERSION,
            run: [1; 16],
            request: 1,
            actor: 0,
            command: action.into(),
        }
    }
    #[test]
    fn gates_reject_duplicate_missing_and_out_of_range_participants() {
        for actors in [vec![0], vec![0, 0], vec![0, 32]] {
            assert!(
                request(Action::ConfigureGate { gate: 1, actors })
                    .validate()
                    .is_err()
            );
        }
        assert!(
            request(Action::ConfigureGate {
                gate: 0,
                actors: vec![0, 1]
            })
            .validate()
            .is_err()
        );
        let mut input = request(Action::CloseDatabase {});
        input.command.gate = Some(1);
        assert!(input.validate().is_err());
        assert!(
            request(Action::ConfigureGate {
                gate: 1,
                actors: vec![0, 1]
            })
            .validate()
            .is_ok()
        );
    }
    #[test]
    fn alias_and_statement_limits_are_pre_dispatch_errors() {
        assert!(
            request(Action::Begin {
                transaction: 0,
                mode: Mode::ReadOnly
            })
            .validate()
            .is_err()
        );
        assert!(
            request(Action::NextPage {
                cursor: 1,
                limit: 1025
            })
            .validate()
            .is_err()
        );
        assert!(
            request(Action::Statement {
                transaction: 1,
                mutations: vec![Mutation::Delete { table: 1, row: 1 }; 257]
            })
            .validate()
            .is_err()
        );
        assert!(
            request(Action::RestoreDatabase {
                source: 0,
                target: 0
            })
            .validate()
            .is_err()
        );
    }
    #[test]
    fn transaction_unknown_is_not_collapsed_into_known_abort() {
        let response = Response {
            version: VERSION,
            run: [1; 16],
            request: 1,
            actor: 0,
            passed_gate: None,
            observation: Observation::Error {
                failure: Failure::Transaction {
                    code: "Poisoned".into(),
                    effect: Effect::Unknown,
                    certainty: Certainty::Unknown,
                    retry: Retry::AfterRecovery,
                    transaction: None,
                },
            },
        };
        let bytes = crate::frame::encode(&response).unwrap();
        assert_eq!(
            crate::frame::read::<Response>(&mut bytes.as_slice()).unwrap(),
            Some(response)
        );
    }
    #[test]
    fn maximum_counter_histograms_fit_the_reserved_response_bound() {
        let operations = DURABILITY_METRIC_IDS
            .iter()
            .map(|id| DurabilityOperationMetrics {
                metric_id: (*id).into(),
                attempts: u64::MAX,
                successes: u64::MAX,
                failures: u64::MAX,
                requested_bytes: Some(u64::MAX),
                confirmed_complete_bytes: Some(u64::MAX),
                items: Some(u64::MAX),
                unknown_partial_operations: u64::MAX,
                timing_missing: u64::MAX,
                elapsed: Some(DurabilityTiming {
                    samples: u64::MAX,
                    sum_ns: u64::MAX,
                    max_ns: u64::MAX,
                    log2_ns: vec![u64::MAX; 64],
                }),
            })
            .collect();
        let mut response = Response {
            version: VERSION,
            run: [1; 16],
            request: 1,
            actor: 0,
            passed_gate: None,
            observation: Observation::DurabilityMetrics {
                counters: DurabilityMetrics {
                    semantic_revision: 1,
                    operations: Some(operations),
                    clock_configured: true,
                    clock_invalid: false,
                    counters_saturated: true,
                    gauges: DurabilityGauges {
                        retained_wal_bytes: u64::MAX,
                        durable_position: u64::MAX,
                        checkpoint_position: u64::MAX,
                        control_reserved_bytes: u64::MAX,
                        control_used_bytes: u64::MAX,
                        oldest_system_pin_age_ns: Some(u64::MAX),
                        system_pins_over_60_seconds: Some(u64::MAX),
                    },
                },
            },
        };
        assert!(crate::frame::encode(&response).unwrap().len() <= 256 * 1024);
        if let Observation::DurabilityMetrics { counters } = &mut response.observation {
            counters.operations.as_mut().unwrap()[0]
                .elapsed
                .as_mut()
                .unwrap()
                .log2_ns
                .pop();
        }
        assert!(response.validate().is_err());
    }
}
