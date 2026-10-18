use crate::{
    Error, ErrorCode,
    api::{DatabaseId, Metrics, RowUid, StoreOptions, TableHandle},
};
use std::fmt;
use xadb_foundation::{error::Certainty, value::Value};

/// Non-reused transaction identity within one durably reserved writer epoch.
/// K1 supplies a fixture epoch; actual epoch reservation belongs to K2/K3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxnId {
    /// Database incarnation.
    pub database: DatabaseId,
    /// Restore branch.
    pub timeline: u64,
    /// Writer epoch; nonzero.
    pub writer_epoch: u32,
    /// Begin/DDL issuance number; nonzero and never returned on rollback.
    pub local: u64,
}

/// Immutable read-only snapshots or private writable snapshots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionMode {
    /// Reads a fixed root and needs no validation at completion.
    ReadOnly,
    /// Validates the start position even when no statement changed rows.
    ReadWrite,
}

/// Limits for the volatile transaction fixture; these are logical ledger budgets.
#[derive(Clone, Copy, Debug)]
pub struct TransactionOptions {
    /// Committed/old resident allocation and snapshot pin budgets.
    pub store: StoreOptions,
    /// Concurrent admitted transactions including standalone DDL, 1..=32.
    pub active_transactions: u64,
    /// Shared private allocation pool, at most 64 MiB.
    pub private_bytes: u64,
    /// Private allocation limit per transaction, at most 8 MiB.
    pub transaction_bytes: u64,
}
impl Default for TransactionOptions {
    fn default() -> Self {
        Self {
            store: StoreOptions::default(),
            active_transactions: 32,
            private_bytes: 64 * 1024 * 1024,
            transaction_bytes: 8 * 1024 * 1024,
        }
    }
}

/// Input-order mutations applied atomically as one private statement.
#[derive(Clone, Debug)]
pub enum Mutation {
    /// Issue a provisional RowUID after validating the row.
    Insert {
        /// Table in this transaction's catalog.
        table: TableHandle,
        /// Complete typed row.
        values: Vec<Value>,
    },
    /// Replace an existing row; absence is an error, never an upsert.
    Replace {
        /// Table in this transaction's catalog.
        table: TableHandle,
        /// Stable row identity.
        row: RowUid,
        /// Complete replacement row.
        values: Vec<Value>,
    },
    /// Delete an existing row from the private root.
    Delete {
        /// Table in this transaction's catalog.
        table: TableHandle,
        /// Stable row identity.
        row: RowUid,
    },
}

/// Caller-owned bounded statement output. Its buffer is outside engine ledgers
/// after return, like caller input buffers; inserted identities remain provisional.
#[derive(Debug)]
pub struct StatementResult {
    /// Row identities issued by Insert mutations, in input order.
    pub inserted: Vec<RowUid>,
    /// Number of successfully applied input mutations, including repeated keys.
    pub affected_rows: u64,
}

/// Internal volatile completion, never a durable commit receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VolatileCommit {
    /// Identity issued at begin.
    pub transaction: TxnId,
    /// Published position, or the fixed read-only snapshot's position.
    pub position: u64,
    /// Whether this operation published a new root.
    pub published: bool,
}

/// Transaction-owned failure identities, separate from the unchanged K0 catalog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionErrorCode {
    /// Existing row/catalog/schema/quota error.
    Product(ErrorCode),
    /// A write statement was submitted to a read-only transaction.
    ReadOnly,
    /// Current position no longer equals the writer's start position.
    SerializationConflict,
    /// Cooperative cancellation observed before commit attempt.
    Cancelled,
    /// A shared lock was poisoned; this core cannot resume service.
    Poisoned,
    /// New mutation/admission is fenced pending recovery.
    Fenced,
    /// This transaction was already aborted by cancellation or fencing.
    Closed,
}

/// Effect on the transaction, not a claim about unrelated transactions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionEffect {
    /// Only this statement failed; earlier private changes remain active.
    Active,
    /// All private transaction state was discarded without publication.
    Aborted,
    /// A durable/publication decision cannot be undone or safely inferred.
    Unknown,
}

/// The owner must choose a new transaction or recover; writes are never replayed automatically.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionRetry {
    /// Repeating unchanged input is not advised.
    Never,
    /// A new snapshot and complete transaction retry may be appropriate.
    Transaction,
    /// A new recovered core is required.
    AfterRecovery,
}

/// Typed transaction failure with explicit effect and retry boundaries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransactionError {
    /// Machine-readable identity.
    pub code: TransactionErrorCode,
    /// Static context, never caller-provided text.
    pub context: &'static str,
    /// Whether private state remains active.
    pub effect: TransactionEffect,
    /// Known for pre-attempt failures; Unknown after an unresolved publication.
    pub certainty: Certainty,
    /// Retry boundary.
    pub retry: TransactionRetry,
    /// Issued identity when admission reached identity issuance.
    pub transaction: Option<TxnId>,
}
impl TransactionError {
    pub(crate) fn new(code: TransactionErrorCode, context: &'static str) -> Self {
        Self {
            code,
            context,
            effect: TransactionEffect::Active,
            certainty: Certainty::Known,
            retry: TransactionRetry::Never,
            transaction: None,
        }
    }
    pub(crate) fn id(mut self, id: TxnId) -> Self {
        self.transaction = Some(id);
        self
    }
    pub(crate) fn aborted(mut self) -> Self {
        self.effect = TransactionEffect::Aborted;
        self
    }
    pub(crate) fn unknown(mut self) -> Self {
        self.effect = TransactionEffect::Unknown;
        self.certainty = Certainty::Unknown;
        self.retry = TransactionRetry::AfterRecovery;
        self
    }
}
impl From<Error> for TransactionError {
    fn from(error: Error) -> Self {
        Self::new(TransactionErrorCode::Product(error.code), error.context)
    }
}
impl fmt::Display for TransactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.context)
    }
}
impl std::error::Error for TransactionError {}

/// Transaction operation result.
pub type TransactionResult<T> = std::result::Result<T, TransactionError>;

/// Independent gauges; concurrent observations are not one coherent cut.
#[derive(Clone, Copy, Debug)]
pub struct TransactionMetrics {
    /// Existing row work and resident/pin accounting.
    pub substrate: Metrics,
    /// Current global private leases, counted once per private allocation.
    pub private_bytes: u64,
    /// Current admitted DML and DDL transactions.
    pub active_transactions: u64,
    /// Oldest admitted user pin in the injected clock domain. None with no pins,
    /// without a usable clock or during an incomplete concurrent observation.
    /// Age is diagnostic and never authorizes forcibly releasing an old reader.
    pub oldest_user_pin_age_ns: Option<u64>,
    /// Independently observed pins held for more than 60 seconds; None when age
    /// cannot be observed. DDL's short-lived admission pin is included.
    pub user_pins_over_sixty_seconds: Option<u64>,
    /// Successful coordinator acquisitions, including identity issuance and DDL.
    pub coordinator_acquisitions: u64,
    /// Cumulative wait nanoseconds in the injected clock domain; None without a
    /// clock or after a backward/incomplete clock observation, never invented zero.
    pub coordinator_wait_ns: Option<u64>,
    /// Completed guard hold nanoseconds in the same injected clock domain.
    pub coordinator_hold_ns: Option<u64>,
    /// A backward clock or unwound guard invalidated time accumulation.
    pub coordinator_clock_invalid: bool,
    /// Any fixed transaction counter saturated; exact deltas are no longer valid.
    pub counters_saturated: bool,
    /// Whole writers rejected because their start position was stale.
    pub serialization_conflicts: u64,
    /// Statements rejected after entering staging, including whole-abort errors.
    pub statement_rollbacks: u64,
    /// Resident preparation/position/result/cancellation failures before the hook.
    /// Validation conflicts and durable hook failures are excluded.
    pub commit_preparation_failures: u64,
}
