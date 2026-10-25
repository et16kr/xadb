//! SQL-free DEV2 durable API. A successful write receipt follows file sync and
//! atomic root publication. Reopening is always a current-build recovery operation.
#[cfg(test)]
mod capacity_tests;
mod maintenance;

use crate::{
    api::{DatabaseId, RowUid, TableHandle, TableSpec},
    durable::{
        changes, checkpoint,
        codec::{self, Identity, MAX_FILES, Manifest, ManifestRef, Record},
        error::{DurableError as StorageError, DurableErrorCode as StorageCode},
        io,
        observation::{DurabilityOperation as Operation, Observations, Span},
        recovery,
        retention::{Pin, Retention},
        wal::Wal,
    },
    transaction::{
        CommitWithError, HookFailure, Mutation, PreparedCommit, RecoveryBuildError,
        StatementResult, Transaction, TransactionCursor, TransactionError, TransactionMetrics,
        TransactionMode, TransactionOptions, TransactionResult, TransactionRow, TxnId,
        VolatileDatabase,
    },
};
use foundation::{
    namespace::{DirectoryPort, Selector, WriterNamespace},
    runtime::{Cancellation, Clock, Quota, Reservation},
};
use std::{
    cell::RefCell,
    fmt,
    mem::size_of,
    ops::{Deref, DerefMut},
    sync::{Arc, Mutex, MutexGuard},
};

pub use crate::durable::error::{DurableError, DurableErrorCode, TransactionOutcome};
pub use crate::durable::observation::{
    DURABILITY_OPERATION_COUNT, DurabilityGauges, DurabilityMetrics, DurabilityOperation,
    DurabilityOperationMetrics, DurabilityTiming,
};
pub use maintenance::{BackupReceipt, CheckpointReceipt, GarbageCollection};

/// Configuration for a current-build DEV2 database; these are engineering bounds,
/// not measured product-capacity guarantees.
#[derive(Clone)]
pub struct EngineOptions {
    /// User transactions, private workspace and committed/old snapshot budget.
    /// The resident budget includes an 8 MiB reservation for maintenance metadata;
    /// ordinary row admission cannot consume that reservation.
    pub transactions: TransactionOptions,
    /// Shared WAL/checkpoint/recovery scratch, at most 16 MiB.
    pub workspace_bytes: u64,
    /// Collect fixed durability counters and optional clock durations. Off mode
    /// reports unavailable operations rather than zero work.
    pub observation_enabled: bool,
    /// Optional monotonic observation port. Calls must be nonblocking and must
    /// never reenter this engine. Unavailable timing remains None, never zero.
    pub clock: Option<Arc<dyn Clock + Send + Sync>>,
}
impl Default for EngineOptions {
    fn default() -> Self {
        Self {
            transactions: TransactionOptions::default(),
            workspace_bytes: recovery::SCRATCH_BYTES,
            observation_enabled: true,
            clock: None,
        }
    }
}
impl EngineOptions {
    fn validate(&self) -> Result<()> {
        if !(1..=64 * 1024 * 1024).contains(&self.transactions.store.resident_bytes)
            || !(1..=16 * 1024 * 1024).contains(&self.workspace_bytes)
        {
            return Err(storage(
                StorageError::new(
                    StorageCode::InvalidInput,
                    "engine resident/workspace budget",
                ),
                None,
            ));
        }
        Ok(())
    }
}

/// Complete operation failure; statement and transaction errors retain their
/// own typed effect instead of being collapsed into a generic I/O failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineError {
    /// The transaction owner rejected admission, a statement, or validation.
    Transaction(TransactionError),
    /// Persistent state or a storage boundary failed.
    Storage {
        /// Durable decision and whether reopening is required.
        error: DurableError,
        /// Issued transaction identity, if a transaction was involved.
        transaction: Option<TxnId>,
    },
}
impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transaction(error) => error.fmt(f),
            Self::Storage { error, .. } => error.fmt(f),
        }
    }
}
impl std::error::Error for EngineError {}
impl From<TransactionError> for EngineError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

/// Result of a durable engine operation.
pub type Result<T> = std::result::Result<T, EngineError>;

/// Receipt produced after the transaction's durable/publication decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommitReceipt {
    /// Logical transaction identity, including the current open's writer epoch.
    pub transaction: TxnId,
    /// Fixed snapshot position for a reader; new published position for a writer.
    pub position: u64,
    /// False for read-only and validated transactions with no final change.
    pub wrote: bool,
}

struct Selected {
    reference: ManifestRef,
    manifest: Manifest,
    previous: Option<ManifestRef>,
    _charge: Reservation,
}
const CONTROL_BYTES: u64 = 8 * 1024 * 1024;
struct Control {
    quota: Quota,
    /// Backing reservation in the same resident bound as row/old-root memory.
    _resident: Reservation,
}
impl Control {
    fn new(core: &VolatileDatabase) -> Result<Self> {
        let backing = reserve(&core.resident_quota(), CONTROL_BYTES as usize)?;
        Ok(Self {
            quota: Quota::new(CONTROL_BYTES),
            _resident: backing,
        })
    }
}
struct State<W: WriterNamespace> {
    wal: Wal<W>,
    selected: Selected,
    next_generation: u64,
    /// An uncertain CURRENT selection retains both possible closures until close.
    uncertain_selection: Option<Pin>,
}
struct Shared<W: WriterNamespace> {
    core: VolatileDatabase,
    state: Mutex<State<W>>,
    retention: Arc<Retention>,
    identity: Identity,
    control: Control,
    observations: Arc<Observations>,
    _charge: Reservation,
}

/// Cloneable owner of one database incarnation and writer lease. File handles
/// keep the OS lease alive, and Drop never attempts a hidden commit or sync.
pub struct Engine<W: WriterNamespace> {
    shared: Arc<Shared<W>>,
}
impl<W: WriterNamespace> Clone for Engine<W> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

struct StateGuard<'a, W: WriterNamespace> {
    guard: MutexGuard<'a, State<W>>,
    shared: &'a Shared<W>,
    held: Option<Span<'a>>,
}
impl<W: WriterNamespace> Deref for StateGuard<'_, W> {
    type Target = State<W>;
    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}
impl<W: WriterNamespace> DerefMut for StateGuard<'_, W> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}
impl<W: WriterNamespace> Drop for StateGuard<'_, W> {
    fn drop(&mut self) {
        let success = !std::thread::panicking();
        if !success {
            self.shared.core.fence();
        }
        if let Some(span) = self.held.take() {
            span.finish(success, 0);
        }
    }
}

fn storage(error: StorageError, transaction: Option<TxnId>) -> EngineError {
    EngineError::Storage { error, transaction }
}
fn reserve(quota: &Quota, bytes: usize) -> Result<Reservation> {
    quota
        .reserve(
            u64::try_from(bytes)
                .map_err(|_| storage(StorageError::limit("engine allocation size"), None))?,
        )
        .map_err(|_| storage(StorageError::allocation(), None))
}
fn metadata_bytes(manifest: &Manifest) -> std::result::Result<usize, StorageError> {
    let mut bytes = manifest
        .tables
        .capacity()
        .checked_mul(size_of::<codec::TableImage>())
        .and_then(|n| {
            manifest
                .chunks
                .capacity()
                .checked_mul(size_of::<codec::ChunkRef>())
                .and_then(|chunks| n.checked_add(chunks))
        })
        .ok_or(StorageError::limit("manifest allocation"))?;
    for table in &manifest.tables {
        bytes = bytes
            .checked_add(table.spec.name.capacity())
            .and_then(|n| {
                table
                    .spec
                    .columns
                    .capacity()
                    .checked_mul(size_of::<crate::api::Column>())
                    .and_then(|columns| n.checked_add(columns))
            })
            .ok_or(StorageError::limit("schema allocation"))?;
        for column in &table.spec.columns {
            bytes = bytes
                .checked_add(column.name.capacity())
                .ok_or(StorageError::limit("schema name allocation"))?;
        }
    }
    Ok(bytes)
}

impl<W: WriterNamespace> Engine<W> {
    /// Create a fresh database in an already-existing caller-owned empty directory.
    /// A permanent LOCK file alone is allowed. Existing files are never overwritten.
    ///
    /// # Errors
    /// Busy/invalid input has no database commit. Failed bootstrap may leave an
    /// incomplete creation, which is preserved and must not be implicitly retried.
    pub fn create(
        directory: &impl DirectoryPort<Writer = W>,
        options: EngineOptions,
    ) -> Result<Self> {
        options.validate()?;
        let scratch = Quota::new(options.workspace_bytes);
        let mut observations =
            Observations::new(options.observation_enabled, options.clock.clone(), &scratch)
                .map_err(|e| storage(e, None))?;
        let mut writer = directory
            .acquire_writer()
            .map_err(io::namespace)
            .map_err(|e| storage(e, None))?;
        require_empty(&writer, &scratch)?;
        let database = DatabaseId::new(
            writer
                .random_incarnation()
                .map_err(io::namespace)
                .map_err(|e| storage(e, None))?,
        )
        .map_err(|_| {
            storage(
                StorageError::new(StorageCode::InvalidInput, "zero random database identity"),
                None,
            )
        })?;
        let identity = Identity {
            database,
            timeline: 1,
            epoch: 1,
        };
        let core = match options.clock {
            Some(clock) => {
                VolatileDatabase::new_with_clock(database, 1, 1, options.transactions, clock)?
            }
            None => VolatileDatabase::new(database, 1, 1, options.transactions)?,
        };
        let control = Control::new(&core)?;
        rehome_observations(&mut observations, &control)?;
        let mut wal = Wal::install_epoch(
            writer,
            identity,
            0,
            None,
            false,
            scratch,
            0,
            Arc::clone(&observations),
        )
        .map_err(|e| storage(e, None))?;
        let root = core.committed_root()?;
        let anchor = wal.anchor();
        let scratch = wal.scratch.clone();
        wal.check_files(
            checkpoint::required_files(&root.root, false).map_err(|e| storage(e, None))? + 2,
        )
        .map_err(|e| storage(e, None))?;
        let built = {
            let writer = RefCell::new(&mut wal.writer);
            checkpoint::build(
                &root.root,
                identity,
                1,
                anchor,
                None,
                |name| writer.borrow_mut().create_new(name).map_err(io::namespace),
                || writer.borrow_mut().sync_directory().map_err(io::namespace),
                &scratch,
                &observations,
            )
            .map_err(|e| storage(e, None))?
        };
        let charge = reserve(
            &control.quota,
            metadata_bytes(&built.manifest).map_err(|e| storage(e, None))?,
        )?;
        let selection = codec::encode(
            identity,
            &Record::Current {
                current: built.reference.clone(),
                previous: None,
            },
        )
        .map_err(|e| storage(e, None))?;
        let selecting = observations.start(Operation::CheckpointSelect, selection.len() as u64, 1);
        let result = io::select(
            &mut wal.writer,
            1,
            &mut wal.selector_serial,
            Selector::Current,
            &selection,
        );
        selecting.finish(
            result.is_ok(),
            if result.is_ok() {
                selection.len() as u64
            } else {
                0
            },
        );
        result.map_err(|e| storage(e, None))?;
        wal.refresh_inventory().map_err(|e| storage(e, None))?;
        let selected = Selected {
            reference: built.reference,
            manifest: built.manifest,
            previous: None,
            _charge: charge,
        };
        Self::assemble(core, wal, selected, 2, control, observations)
    }

    /// Recover exactly the files selected by CURRENT/WRITER, stabilize every
    /// adopted dependency, reserve a new epoch, and only then expose normal APIs.
    ///
    /// # Errors
    /// Unsupported/corrupt/incomplete files, quota, busy or I/O errors prevent open.
    /// Original WAL/checkpoint bytes are never truncated, repaired or deleted here.
    pub fn open(
        directory: &impl DirectoryPort<Writer = W>,
        options: EngineOptions,
    ) -> Result<Self> {
        options.validate()?;
        let mut writer = directory
            .acquire_writer()
            .map_err(io::namespace)
            .map_err(|e| storage(e, None))?;
        let scratch = Quota::new(options.workspace_bytes);
        let mut observations =
            Observations::new(options.observation_enabled, options.clock.clone(), &scratch)
                .map_err(|e| storage(e, None))?;
        let plan = recovery::inspect(&mut writer, &scratch, &observations)
            .map_err(|e| storage(e, None))?;
        let identity = Identity {
            epoch: plan.next_epoch,
            ..plan.identity
        };
        let core = VolatileDatabase::from_recovered_with_clock(
            identity.database,
            identity.timeline,
            identity.epoch,
            options.transactions,
            plan.tip.q,
            options.clock,
            |quota| recovery::apply(&plan, &mut writer, quota, &observations),
        )
        .map_err(|e| match e {
            RecoveryBuildError::Transaction(e) => EngineError::Transaction(e),
            RecoveryBuildError::Build(e) => storage(e, None),
        })?;
        let control = Control::new(&core)?;
        rehome_observations(&mut observations, &control)?;
        recovery::stabilize(&plan, &mut writer, &observations).map_err(|e| storage(e, None))?;
        let mut selector_serial = 0;
        if plan.fallback_used {
            let bytes = codec::encode(
                identity,
                &Record::Current {
                    current: plan.checkpoint_ref.clone(),
                    previous: None,
                },
            )
            .map_err(|e| storage(e, None))?;
            let selecting = observations.start(Operation::CheckpointSelect, bytes.len() as u64, 1);
            let result = io::select(
                &mut writer,
                identity.epoch,
                &mut selector_serial,
                Selector::Current,
                &bytes,
            );
            selecting.finish(
                result.is_ok(),
                if result.is_ok() {
                    bytes.len() as u64
                } else {
                    0
                },
            );
            result.map_err(|e| storage(e, None))?;
        }
        let charge = reserve(
            &control.quota,
            metadata_bytes(&plan.checkpoint).map_err(|e| storage(e, None))?,
        )?;
        let (_, manifest, reference, previous, tip, _, next_generation, fallback) =
            plan.into_checkpoint_parts();
        let selected = Selected {
            reference,
            manifest,
            previous: if fallback { None } else { previous },
            _charge: charge,
        };
        let wal = Wal::install_epoch(
            writer,
            identity,
            tip.q,
            Some(tip),
            false,
            scratch,
            selector_serial,
            Arc::clone(&observations),
        )
        .map_err(|e| storage(e, None))?;
        Self::assemble(core, wal, selected, next_generation, control, observations)
    }

    fn assemble(
        core: VolatileDatabase,
        wal: Wal<W>,
        selected: Selected,
        next_generation: u64,
        control: Control,
        observations: Arc<Observations>,
    ) -> Result<Self> {
        let retention = Retention::new_with_observations(&control.quota, Arc::clone(&observations))
            .map_err(|e| storage(e, None))?;
        let charge = reserve(
            &control.quota,
            size_of::<Shared<W>>() + 2 * size_of::<usize>(),
        )?;
        let identity = wal.identity;
        Ok(Self {
            shared: Arc::new(Shared {
                core,
                state: Mutex::new(State {
                    wal,
                    selected,
                    next_generation,
                    uncertain_selection: None,
                }),
                retention,
                identity,
                control,
                observations,
                _charge: charge,
            }),
        })
    }

    fn state(&self) -> Result<StateGuard<'_, W>> {
        self.observe_poison();
        let waiting = self
            .shared
            .observations
            .start(Operation::NamespaceWait, 0, 1);
        let result = self.shared.state.lock();
        waiting.finish(result.is_ok(), 0);
        let guard = result.map_err(|_| {
            self.shared.core.fence();
            storage(
                StorageError {
                    fenced: true,
                    ..StorageError::new(StorageCode::Fenced, "persistence coordinator poisoned")
                },
                None,
            )
        })?;
        Ok(StateGuard {
            guard,
            shared: &self.shared,
            held: Some(
                self.shared
                    .observations
                    .start(Operation::NamespaceHold, 0, 1),
            ),
        })
    }

    fn observe_poison(&self) {
        if self.shared.state.is_poisoned() || self.shared.retention.is_poisoned() {
            self.shared.core.fence();
        }
    }

    fn ready(&self) -> Result<()> {
        self.observe_poison();
        if self.shared.core.is_fenced() {
            return Err(storage(
                StorageError {
                    fenced: true,
                    ..StorageError::new(StorageCode::Fenced, "engine requires recovery")
                },
                None,
            ));
        }
        Ok(())
    }

    fn persist(
        &self,
        prepared: &PreparedCommit,
    ) -> std::result::Result<(), HookFailure<EngineError>> {
        let execute = || -> Result<()> {
            self.ready()?;
            let mut state = self.state()?;
            if prepared.id.database != state.wal.identity.database
                || prepared.id.timeline != state.wal.identity.timeline
                || prepared.id.writer_epoch != state.wal.identity.epoch
            {
                return Err(storage(
                    StorageError {
                        fenced: true,
                        ..StorageError::invalid("prepared transaction namespace")
                    },
                    Some(prepared.id),
                ));
            }
            let batch = changes::prepare(prepared, &state.wal.scratch)
                .map_err(|e| storage(e, Some(prepared.id)))?;
            state
                .wal
                .append(&batch.batch, &prepared.cancellation)
                .map_err(|mut e| {
                    if e.outcome == TransactionOutcome::NotAttempted {
                        e.outcome = TransactionOutcome::Aborted;
                    }
                    storage(e, Some(prepared.id))
                })
        };
        execute().map_err(|mut error| {
            if let EngineError::Storage { error, .. } = &mut error
                && error.outcome == TransactionOutcome::NotAttempted
            {
                error.outcome = TransactionOutcome::Aborted;
            }
            let fence = matches!(&error, EngineError::Storage { error, .. } if error.fenced);
            HookFailure { error, fence }
        })
    }

    fn commit_result<T>(
        &self,
        result: std::result::Result<T, CommitWithError<EngineError>>,
    ) -> Result<T> {
        result.map_err(|error| match error {
            CommitWithError::Transaction(error) => EngineError::Transaction(error),
            CommitWithError::Hook(error) => error,
            CommitWithError::Publication(error) => {
                self.shared.core.fence();
                storage(
                    StorageError {
                        outcome: TransactionOutcome::Committed,
                        fenced: true,
                        ..StorageError::new(
                            StorageCode::Fenced,
                            "durable decision survived publication failure",
                        )
                    },
                    error.transaction,
                )
            }
        })
    }

    /// Current database incarnation, retained across normal restart and restore.
    pub fn database_id(&self) -> DatabaseId {
        self.shared.identity.database
    }
    /// Restore branch namespace; normal restart preserves it.
    pub fn timeline(&self) -> u64 {
        self.shared.identity.timeline
    }
    /// True when writes/admission require recovery in a new instance.
    pub fn is_fenced(&self) -> bool {
        self.observe_poison();
        self.shared.core.is_fenced()
    }
    /// Current transaction/row/resource observations; unavailable clock values are None.
    pub fn metrics(&self) -> TransactionMetrics {
        self.shared.core.metrics()
    }

    /// Observe fixed durability counters, inclusive duration histograms and
    /// resource gauges. A snapshot of atomic counters is not a coherent event cut.
    ///
    /// # Errors
    /// Poisoned namespace or retention ownership requires reopening the engine.
    pub fn durability_metrics(&self) -> Result<DurabilityMetrics> {
        let state = self.state()?;
        let (age, overdue) = self.shared.retention.pin_diagnostics().map_err(|e| {
            self.shared.core.fence();
            storage(e, None)
        })?;
        let gauges = DurabilityGauges {
            retained_wal_bytes: state.wal.retained_wal_bytes,
            durable_position: state.wal.position,
            checkpoint_position: state.selected.manifest.cut,
            control_reserved_bytes: CONTROL_BYTES,
            control_used_bytes: self.shared.control.quota.used(),
            oldest_system_pin_age_ns: age,
            system_pins_over_60_seconds: overdue,
        };
        drop(state);
        Ok(self.shared.observations.snapshot(gauges))
    }

    /// Begin a fixed-snapshot transaction with bounded admission and private state.
    ///
    /// # Errors
    /// Fenced/poisoned state, resource or identity exhaustion admits no transaction.
    pub fn begin(&self, mode: TransactionMode) -> Result<DurableTransaction<W>> {
        self.ready()?;
        Ok(DurableTransaction {
            inner: self.shared.core.begin(mode)?,
            engine: self.clone(),
        })
    }

    /// Durably create a table as one independent catalog transaction.
    ///
    /// # Errors
    /// Validation/preparation failures leave the catalog unchanged; write/sync
    /// failures preserve Unknown and fence instead of claiming rollback.
    pub fn create_table(&self, spec: TableSpec) -> Result<TableHandle> {
        self.create_table_with_receipt(spec).map(|(table, _)| table)
    }

    /// Create a table and return the exact implicit DDL transaction identity and
    /// commit position after WAL sync and root publication. The receipt is captured
    /// from this transaction's prepared state, never from a later metrics sample.
    ///
    /// # Errors
    /// Uses the same validation, uncertainty and fencing rules as create_table.
    pub fn create_table_with_receipt(
        &self,
        spec: TableSpec,
    ) -> Result<(TableHandle, CommitReceipt)> {
        self.ready()?;
        let mut receipt = None;
        let table = self.commit_result(self.shared.core.create_table_with(spec, |prepared| {
            receipt = Some(CommitReceipt {
                transaction: prepared.id,
                position: prepared.position,
                wrote: true,
            });
            self.persist(prepared)
        }))?;
        Ok((table, self.ddl_receipt(receipt)?))
    }

    /// Durably drop a table. Old transactions keep their prior catalog and rows.
    ///
    /// # Errors
    /// Stale handles or preparation errors do not publish; uncertain I/O fences.
    pub fn drop_table(&self, table: &TableHandle) -> Result<()> {
        self.drop_table_with_receipt(table).map(|_| ())
    }

    /// Drop a table with the exact implicit DDL commit receipt. Existing readers
    /// keep their prior catalog while the receipt identifies this durable change.
    ///
    /// # Errors
    /// Uses the same validation, uncertainty and fencing rules as drop_table.
    pub fn drop_table_with_receipt(&self, table: &TableHandle) -> Result<CommitReceipt> {
        self.ready()?;
        let mut receipt = None;
        self.commit_result(self.shared.core.drop_table_with(table, |prepared| {
            receipt = Some(CommitReceipt {
                transaction: prepared.id,
                position: prepared.position,
                wrote: true,
            });
            self.persist(prepared)
        }))?;
        self.ddl_receipt(receipt)
    }

    fn ddl_receipt(&self, receipt: Option<CommitReceipt>) -> Result<CommitReceipt> {
        receipt.ok_or_else(|| {
            self.shared.core.fence();
            storage(
                StorageError {
                    outcome: TransactionOutcome::Committed,
                    fenced: true,
                    ..StorageError::invalid("published DDL lacks its prepared receipt")
                },
                None,
            )
        })
    }
}

fn rehome_observations(observations: &mut Arc<Observations>, control: &Control) -> Result<()> {
    Arc::get_mut(observations)
        .ok_or_else(|| {
            storage(
                StorageError::invalid("observation owner shared before admission"),
                None,
            )
        })?
        .rehome(&control.quota)
        .map_err(|e| storage(e, None))
}

fn require_empty<W: WriterNamespace>(writer: &W, quota: &Quota) -> Result<()> {
    let _charge = reserve(
        quota,
        MAX_FILES * size_of::<foundation::namespace::DirectoryEntry>(),
    )?;
    let entries = writer
        .list_owned(MAX_FILES)
        .map_err(io::namespace)
        .map_err(|e| storage(e, None))?;
    let extra = entries
        .capacity()
        .saturating_sub(MAX_FILES)
        .checked_mul(size_of::<foundation::namespace::DirectoryEntry>())
        .ok_or_else(|| {
            storage(
                StorageError::limit("empty directory inventory capacity"),
                None,
            )
        })?;
    let _extra_charge = reserve(quota, extra)?;
    if entries.iter().any(|entry| entry.name.as_str() != "LOCK") {
        return Err(storage(
            StorageError::new(
                StorageCode::InvalidInput,
                "creation requires an empty owned directory",
            ),
            None,
        ));
    }
    Ok(())
}

/// A transaction whose inner volatile commit path is inaccessible to callers.
/// Rows/cursors borrow this owner, preventing use after commit or rollback.
pub struct DurableTransaction<W: WriterNamespace> {
    inner: Transaction,
    engine: Engine<W>,
}
impl<W: WriterNamespace> DurableTransaction<W> {
    /// Issued transaction identity; it is not a request deduplication key.
    pub fn id(&self) -> TxnId {
        self.inner.id()
    }
    /// Cooperative cancellation handle; a commit already attempted is not undone.
    pub fn cancellation(&self) -> Cancellation {
        self.inner.cancellation()
    }
    /// Look up a table in this transaction's fixed catalog.
    /// # Errors
    /// Invalid/missing names or a closed transaction are explicit errors.
    pub fn lookup_table(&self, name: &str) -> TransactionResult<TableHandle> {
        self.inner.lookup_table(name)
    }
    /// Borrow the schema retained by this transaction.
    /// # Errors
    /// Stale/wrong-instance handles and closed transactions are rejected.
    pub fn schema(&self, table: &TableHandle) -> TransactionResult<&TableSpec> {
        self.inner.schema(table)
    }
    /// Read the fixed snapshot plus successful private statements.
    /// # Errors
    /// Wrong namespace/table handles or a closed transaction are rejected.
    pub fn read(
        &self,
        table: &TableHandle,
        row: RowUid,
    ) -> TransactionResult<Option<TransactionRow<'_>>> {
        self.inner.read(table, row)
    }
    /// Open an ordered scan borrowing this transaction and its private view.
    /// # Errors
    /// Stale handles or a closed transaction are rejected.
    pub fn scan(&self, table: &TableHandle) -> TransactionResult<TransactionCursor<'_>> {
        self.inner.scan(table)
    }
    /// Apply one complete private statement, or preserve its entry state on error.
    /// # Errors
    /// Type/row/quota errors affect only the statement; cancellation may abort all.
    pub fn execute_statement(
        &mut self,
        mutations: Vec<Mutation>,
    ) -> TransactionResult<StatementResult> {
        self.engine.observe_poison();
        self.inner.execute_statement(mutations)
    }
    /// Validate, prepare, write and sync once, atomically publish, then return.
    /// # Errors
    /// Validation/preparation aborts; post-attempt uncertainty fences. No automatic
    /// replay occurs, and a synced decision cannot be rolled back by later errors.
    pub fn commit(self) -> Result<CommitReceipt> {
        let Self { inner, engine } = self;
        engine.observe_poison();
        let receipt =
            engine.commit_result(inner.commit_with(|prepared| engine.persist(prepared)))?;
        Ok(CommitReceipt {
            transaction: receipt.transaction,
            position: receipt.position,
            wrote: receipt.published,
        })
    }
    /// Discard private state and pins without any commit or storage effect.
    pub fn rollback(self) {
        self.inner.rollback();
    }
}
