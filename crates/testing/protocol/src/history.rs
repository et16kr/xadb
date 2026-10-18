//! P0 controller-owned row histories. These are observations, not engine API types.
//! No wire decoder or native worker is implied by these in-process data structures.
use std::collections::BTreeMap;

/// Logical table and row numbers; both must be nonzero. Never an address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key {
    pub table: u64,
    pub row: u64,
}

/// Plain independent scalar representation; no production comparison or codec.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Cell {
    Null,
    Bool(bool),
    Int64(i64),
    Utf8(String),
    Binary(Vec<u8>),
}

pub type Row = Vec<Cell>;
pub type Rows = BTreeMap<Key, Row>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mode {
    ReadOnly,
    ReadWrite,
}

/// Controller's predeclared fault choice, never a worker's claimed witness.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CommitFault {
    None,
    BeforeAttempt,
    Uncertain,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mutation {
    Insert(Key, Row),
    Replace(Key, Row),
    Delete(Key),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Action {
    Begin(Mode),
    Read(Key),
    Scan(u64),
    Statement(Vec<Mutation>),
    Commit(CommitFault),
    Rollback,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rejection {
    NoTransaction,
    AlreadyActive,
    ReadOnly,
    RowNotFound,
    DuplicateRow,
    SerializationConflict,
    Fenced,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Observation {
    Begun,
    Row(Option<Row>),
    Scan(Vec<(Key, Row)>),
    StatementApplied,
    Committed,
    RolledBack,
    Aborted,
    Unknown,
    Rejected(Rejection),
}

/// A controller event. Invocation and return are separate to preserve overlap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Invoke {
        actor: u8,
        request: u64,
        action: Action,
    },
    Return {
        actor: u8,
        request: u64,
        observation: Observation,
    },
    /// Owned-worker loss followed by model reopen; no power-loss claim.
    Crash,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct History {
    /// Trusted fixture state, independently constructed before candidate execution.
    pub initial: Rows,
    pub events: Vec<Event>,
}
