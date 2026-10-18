//! Versioned native K0 observations, separate from the DEV0 fixture protocol.
//! Aliases select worker-owned capabilities; numeric identities are observations only.
use crate::frame::{Envelope, Error};
use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 1;
pub const MAX_REQUESTS: u64 = 1024;
pub const MAX_PENDING: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Cell {
    Null,
    Bool(bool),
    Int64(i64),
    Utf8(String),
    Binary(Vec<u8>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataType {
    Bool,
    Int64,
    Utf8,
    Binary,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Column {
    pub name: String,
    pub data_type: DataType,
    pub nullable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableSpec {
    pub name: String,
    pub columns: Vec<Column>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Initialize {
        database: [u8; 16],
        timeline: u64,
        epoch: u32,
        resident_bytes: u64,
        snapshot_pins: u64,
    },
    Create {
        table: u64,
        spec: TableSpec,
    },
    Lookup {
        snapshot: u64,
        table: u64,
        name: String,
    },
    DropTable {
        table: u64,
    },
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
    Snapshot {
        snapshot: u64,
    },
    ReleaseSnapshot {
        snapshot: u64,
    },
    Read {
        snapshot: u64,
        table: u64,
        row: u64,
    },
    OpenScan {
        snapshot: u64,
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
    Metrics {},
    /// One isolated fixture batch immediately after Initialize.
    Performance {
        input: crate::performance::Input,
    },
}

/// Identity excludes addresses and the nonserializable open-instance capability.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableIdentity {
    pub database: [u8; 16],
    pub timeline: u64,
    pub object: u64,
    pub generation: u64,
    pub schema_epoch: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RowIdentity {
    pub table: TableIdentity,
    pub row: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Row {
    pub identity: RowIdentity,
    pub values: Vec<Cell>,
}

/// These scalar counters are diagnostics, never an independent correctness verdict.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metrics {
    pub operations: u64,
    pub rows_read: u64,
    pub row_references_copied: u64,
    pub resident_bytes: u64,
    pub snapshot_pins: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub enum Observation {
    Ready {},
    Done {},
    Table {
        identity: TableIdentity,
        spec: TableSpec,
    },
    Inserted {
        identity: RowIdentity,
    },
    Row {
        value: Option<Row>,
    },
    Page {
        rows: Vec<Row>,
        eof: bool,
        canonical_bytes: u32,
    },
    Metrics {
        counters: Metrics,
    },
    Performance {
        output: Box<crate::performance::Output>,
    },
    /// Product failure: public root unchanged, known result, no automatic retry.
    /// Only K0's nine assigned codes are permitted. Protocol failures terminate the stream.
    ProductError {
        code: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u32,
    pub run: [u8; 16],
    pub request: u64,
    pub actor: u8,
    pub action: Action,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub version: u32,
    pub run: [u8; 16],
    pub request: u64,
    pub actor: u8,
    pub observation: Observation,
}

fn validate_identity(version: u32, run: [u8; 16], request: u64, actor: u8) -> Result<(), Error> {
    if version != VERSION
        || run == [0; 16]
        || !(1..=MAX_REQUESTS).contains(&request)
        || usize::from(actor) >= MAX_PENDING
    {
        return Err(Error::Invalid(
            "unsupported version or invalid envelope identity",
        ));
    }
    Ok(())
}

impl Envelope for Request {
    fn validate(&self) -> Result<(), Error> {
        validate_identity(self.version, self.run, self.request, self.actor)
    }
}

impl Envelope for Response {
    fn validate(&self) -> Result<(), Error> {
        validate_identity(self.version, self.run, self.request, self.actor)?;
        if let Observation::ProductError { code } = &self.observation
            && !matches!(
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
            )
        {
            return Err(Error::Invalid("unknown K0 product error"));
        }
        Ok(())
    }
}
