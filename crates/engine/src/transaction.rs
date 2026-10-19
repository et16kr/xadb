//! K1 private transactions and volatile publication. No public method here is a
//! durable receipt. The final Engine owns an unexposed Transaction inside its
//! durable wrapper and uses the crate-private commit hook under the coordinator.
//! Statement rollback drops only staged allocations and never allocates.

mod metrics;
mod read;
#[cfg(test)]
mod tests;
mod types;
pub use read::{TransactionCursor, TransactionPage, TransactionRow};
pub use types::{
    Mutation, StatementResult, TransactionEffect, TransactionError, TransactionErrorCode,
    TransactionMetrics, TransactionMode, TransactionOptions, TransactionResult, TransactionRetry,
    TxnId, VolatileCommit,
};

use metrics::TransactionCounters;

use crate::{
    Error, ErrorCode,
    api::{DatabaseId, Instance, Namespace, RowUid, TableHandle, TableSpec},
    catalog::{self, Root, Schema, TableRoot, check_row, table_index},
    identity::Issuer,
    metrics::Counters,
    publication::{CommittedRoot, Publication},
    storage::volatile::{ARC_HEADER_BYTES, AllocationBudget, StoredRow, array_in, reserve},
};
use std::{
    convert::Infallible,
    mem::size_of,
    ops::{Deref, DerefMut},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
};
use xadb_foundation::{
    identity::{Generation, ObjectHandle, SchemaEpoch},
    runtime::{Cancellation, Quota, Reservation},
};

struct Coordinator {
    issuer: Issuer,
    writer_epoch: u32,
    transaction_local: u64,
}
impl Coordinator {
    fn transaction_id(&mut self, namespace: Namespace) -> TransactionResult<TxnId> {
        self.transaction_local = self.transaction_local.checked_add(1).ok_or_else(|| {
            TransactionError::from(Error::new(
                ErrorCode::IdentityExhausted,
                "transaction identity exhausted",
            ))
        })?;
        Ok(TxnId {
            database: namespace.database,
            timeline: namespace.timeline,
            writer_epoch: self.writer_epoch,
            local: self.transaction_local,
        })
    }
}

struct Shared {
    instance: Arc<Instance>,
    coordinator: Mutex<Coordinator>,
    publication: Publication,
    resident: Quota,
    pins: Quota,
    admitted: Quota,
    private_pool: Quota,
    transaction_bytes: u64,
    counters: Arc<Counters>,
    transaction_counters: TransactionCounters,
    fenced: AtomicBool,
    _charge: Reservation,
}
impl Shared {
    fn open(&self) -> TransactionResult<()> {
        if self.fenced.load(Ordering::Acquire) {
            let mut error =
                TransactionError::new(TransactionErrorCode::Fenced, "core requires recovery")
                    .aborted();
            error.retry = TransactionRetry::AfterRecovery;
            Err(error)
        } else {
            Ok(())
        }
    }
    fn fence(&self) {
        self.fenced.store(true, Ordering::Release);
    }
    fn coordinator(&self) -> TransactionResult<CoordinatorGuard<'_>> {
        let waiting = self.transaction_counters.now();
        let guard = self.coordinator.lock().map_err(|_| {
            self.fence();
            let mut error =
                TransactionError::new(TransactionErrorCode::Poisoned, "coordinator poisoned")
                    .aborted();
            error.retry = TransactionRetry::AfterRecovery;
            error
        })?;
        let mut coordinator = CoordinatorGuard {
            guard,
            shared: self,
            acquired: None,
        };
        coordinator.acquired = self.transaction_counters.now();
        self.transaction_counters
            .acquired(waiting, coordinator.acquired);
        Ok(coordinator)
    }
    fn snapshot(&self) -> TransactionResult<Arc<CommittedRoot>> {
        self.publication.snapshot().map_err(|error| {
            self.fence();
            error.aborted()
        })
    }
    fn admission(self: &Arc<Self>) -> TransactionResult<(Reservation, UserPin)> {
        self.open()?;
        let active = self.admitted.reserve(1).map_err(|_| {
            TransactionError::from(Error::new(
                ErrorCode::QuotaExceeded,
                "active transaction quota",
            ))
            .aborted()
        })?;
        let pin = self.pins.reserve(1).map_err(|_| {
            TransactionError::from(Error::new(
                ErrorCode::QuotaExceeded,
                "transaction snapshot pin quota",
            ))
            .aborted()
        })?;
        let slot = self.transaction_counters.pin().ok_or_else(|| {
            self.fence();
            TransactionError::new(
                TransactionErrorCode::Poisoned,
                "user pin registry exhausted after quota reservation",
            )
            .aborted()
        })?;
        Ok((
            active,
            UserPin {
                shared: Arc::clone(self),
                slot,
                _reservation: pin,
            },
        ))
    }
}

struct UserPin {
    shared: Arc<Shared>,
    slot: usize,
    _reservation: Reservation,
}
impl Drop for UserPin {
    fn drop(&mut self) {
        self.shared.transaction_counters.unpin(self.slot);
    }
}

/// Fencing occurs during unwind, before the underlying mutex becomes poisoned,
/// so existing writers cannot start another private statement after hook panic.
struct CoordinatorGuard<'shared> {
    guard: MutexGuard<'shared, Coordinator>,
    shared: &'shared Shared,
    acquired: Option<u64>,
}
impl Deref for CoordinatorGuard<'_> {
    type Target = Coordinator;
    fn deref(&self) -> &Coordinator {
        &self.guard
    }
}
impl DerefMut for CoordinatorGuard<'_> {
    fn deref_mut(&mut self) -> &mut Coordinator {
        &mut self.guard
    }
}
impl Drop for CoordinatorGuard<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.shared.fence();
            self.shared.transaction_counters.invalidate_clock();
        } else {
            self.shared.transaction_counters.released(self.acquired);
        }
    }
}

/// Shared volatile fixture. Clones share coordinator, namespace, quotas and roots.
/// Dropping a handle never commits; transactions and borrowed views own lifetimes.
#[derive(Clone)]
pub struct VolatileDatabase {
    shared: Arc<Shared>,
}

/// Fully prepared before/after state, visible only to crate-level durable owners.
/// The callback runs under the coordinator, outside the publication lock.
pub(crate) struct PreparedCommit {
    pub id: TxnId,
    pub position: u64,
    pub before: Arc<Root>,
    pub after: Arc<Root>,
    /// The durable owner checks this after encoding/rotation, immediately before
    /// its first commit write, and ignores cancellation after that attempt.
    pub cancellation: Cancellation,
}

/// `fence=false` is legal only before CommitAttempted with no storage effect.
/// Any attempted commit write error must fence; its owner preserves Unknown in E.
pub(crate) struct HookFailure<E> {
    pub error: E,
    pub fence: bool,
}

pub(crate) enum CommitWithError<E> {
    Transaction(TransactionError),
    Hook(E),
    /// The hook succeeded, so a publication failure cannot undo its decision.
    Publication(TransactionError),
}
impl<E> From<TransactionError> for CommitWithError<E> {
    fn from(error: TransactionError) -> Self {
        Self::Transaction(error.aborted())
    }
}

/// Keep durable decode/build errors distinct from core admission/allocation errors.
pub(crate) enum RecoveryBuildError<E> {
    Transaction(TransactionError),
    Build(E),
}
impl<E> From<TransactionError> for RecoveryBuildError<E> {
    fn from(error: TransactionError) -> Self {
        Self::Transaction(error.aborted())
    }
}

fn volatile_result<T>(
    result: std::result::Result<T, CommitWithError<Infallible>>,
) -> TransactionResult<T> {
    result.map_err(|error| match error {
        CommitWithError::Transaction(error) | CommitWithError::Publication(error) => error,
        CommitWithError::Hook(never) => match never {},
    })
}

impl VolatileDatabase {
    /// Construct a fresh volatile namespace. No files or durable epoch are created.
    ///
    /// # Errors
    /// Invalid resource/identity settings or insufficient initial resident quota.
    pub fn new(
        database: DatabaseId,
        timeline: u64,
        epoch: u32,
        options: TransactionOptions,
    ) -> TransactionResult<Self> {
        Self::from_recovered(database, timeline, epoch, options, 0, Root::empty).map_err(|error| {
            match error {
                RecoveryBuildError::Transaction(error) => error,
                RecoveryBuildError::Build(error) => TransactionError::from(error).aborted(),
            }
        })
    }

    /// Construct a volatile fixture with one caller-supplied monotonic clock for
    /// coordinator wait/hold observations. Clock calls must be nonblocking and
    /// must not reenter this database; acquisition/release calls hold coordinator.
    /// A clock panic invalidates timing only and cannot poison transaction state.
    ///
    /// # Errors
    /// Invalid resource/identity settings or insufficient initial resident quota.
    pub fn new_with_clock(
        database: DatabaseId,
        timeline: u64,
        epoch: u32,
        options: TransactionOptions,
        clock: Arc<dyn xadb_foundation::runtime::Clock + Send + Sync>,
    ) -> TransactionResult<Self> {
        Self::from_recovered_with_clock(
            database,
            timeline,
            epoch,
            options,
            0,
            Some(clock),
            Root::empty,
        )
        .map_err(|error| match error {
            RecoveryBuildError::Transaction(error) => error,
            RecoveryBuildError::Build(error) => TransactionError::from(error).aborted(),
        })
    }

    /// Build validated recovered rows using this core's resident quota before any
    /// published root becomes observable. The caller owns format/chain validation.
    pub(crate) fn from_recovered<E>(
        database: DatabaseId,
        timeline: u64,
        epoch: u32,
        options: TransactionOptions,
        position: u64,
        build_root: impl FnOnce(&Quota) -> std::result::Result<Arc<Root>, E>,
    ) -> std::result::Result<Self, RecoveryBuildError<E>> {
        Self::from_recovered_with_clock(
            database, timeline, epoch, options, position, None, build_root,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_recovered_with_clock<E>(
        database: DatabaseId,
        timeline: u64,
        epoch: u32,
        options: TransactionOptions,
        position: u64,
        clock: Option<Arc<dyn xadb_foundation::runtime::Clock + Send + Sync>>,
        build_root: impl FnOnce(&Quota) -> std::result::Result<Arc<Root>, E>,
    ) -> std::result::Result<Self, RecoveryBuildError<E>> {
        if timeline == 0
            || epoch == 0
            || !(1..=32).contains(&options.active_transactions)
            || !(1..=32).contains(&options.store.snapshot_pins)
            || !(1..=64 * 1024 * 1024).contains(&options.private_bytes)
            || !(1..=8 * 1024 * 1024).contains(&options.transaction_bytes)
        {
            return Err(TransactionError::from(Error::new(
                ErrorCode::InvalidInput,
                "transaction options/namespace",
            ))
            .aborted()
            .into());
        }
        let resident = Quota::new(options.store.resident_bytes);
        let instance = Arc::new(Instance {
            namespace: Namespace { database, timeline },
            _charge: reserve(&resident, size_of::<Instance>() + ARC_HEADER_BYTES)
                .map_err(|error| TransactionError::from(error).aborted())?,
        });
        let root = build_root(&resident).map_err(RecoveryBuildError::Build)?;
        let published =
            CommittedRoot::prepare(position, root, &resident).map_err(TransactionError::aborted)?;
        let counters =
            Counters::new(&resident).map_err(|error| TransactionError::from(error).aborted())?;
        let charge = reserve(&resident, size_of::<Shared>() + ARC_HEADER_BYTES)
            .map_err(|error| TransactionError::from(error).aborted())?;
        Ok(Self {
            shared: Arc::new(Shared {
                instance,
                coordinator: Mutex::new(Coordinator {
                    issuer: Issuer::new(epoch)
                        .map_err(|error| TransactionError::from(error).aborted())?,
                    writer_epoch: epoch,
                    transaction_local: 0,
                }),
                publication: Publication::new(published),
                resident,
                pins: Quota::new(options.store.snapshot_pins),
                admitted: Quota::new(options.active_transactions),
                private_pool: Quota::new(options.private_bytes),
                transaction_bytes: options.transaction_bytes,
                counters,
                transaction_counters: TransactionCounters::new(clock),
                fenced: AtomicBool::new(false),
                _charge: charge,
            }),
        })
    }

    /// Admit a transaction and fix one committed root. Issued transaction IDs are
    /// never reused, including a failed later pin/root acquisition.
    ///
    /// # Errors
    /// Fencing, poison, quota or identity exhaustion admits no usable transaction.
    pub fn begin(&self, mode: TransactionMode) -> TransactionResult<Transaction> {
        let (admission, pin) = self.shared.admission()?;
        let id = {
            let mut coordinator = self.shared.coordinator()?;
            self.shared.open()?;
            coordinator
                .transaction_id(self.shared.instance.namespace)
                .map_err(TransactionError::aborted)?
        };
        let start = self.shared.snapshot().map_err(|error| error.id(id))?;
        self.shared.open().map_err(|error| error.id(id))?;
        Ok(Transaction {
            shared: Arc::clone(&self.shared),
            id,
            mode,
            cancellation: Cancellation::default(),
            active: Some(Active {
                root: Arc::clone(&start.root),
                start_position: start.position,
                private: AllocationBudget::private(
                    Quota::new(self.shared.transaction_bytes),
                    self.shared.private_pool.clone(),
                ),
                successful_mutations: 0,
                dirty: false,
                _admission: admission,
                _pin: pin,
            }),
        })
    }

    /// Create one table as an independent volatile catalog transaction.
    ///
    /// # Errors
    /// Invalid schema/name, duplicate/table limits, quota, fencing or poison leave
    /// the public root unchanged. Issued object/transaction numbers stay consumed.
    pub fn create_table(&self, spec: TableSpec) -> TransactionResult<TableHandle> {
        volatile_result(self.create_table_with(spec, |_| Ok(())))
    }

    /// Drop one table as an independent volatile transaction; old readers retain it.
    ///
    /// # Errors
    /// Stale handle, quota, fencing or poison leaves the public root unchanged.
    pub fn drop_table(&self, table: &TableHandle) -> TransactionResult<()> {
        volatile_result(self.drop_table_with(table, |_| Ok(())))
    }

    pub(crate) fn create_table_with<E>(
        &self,
        spec: TableSpec,
        before_publish: impl FnOnce(&PreparedCommit) -> std::result::Result<(), HookFailure<E>>,
    ) -> std::result::Result<TableHandle, CommitWithError<E>> {
        self.ddl_with(
            move |current, coordinator| {
                self.shared.counters.operation();
                catalog::validate_spec(&spec)?;
                if current.name(&spec.name).is_some() {
                    return Err(Error::new(ErrorCode::TableExists, "table name exists").into());
                }
                if current.tables.len() >= 64 {
                    return Err(Error::new(ErrorCode::LimitExceeded, "64 table limit").into());
                }
                let identity = ObjectHandle {
                    object: coordinator.issuer.object()?,
                    generation: Generation::new(1).map_err(|_| {
                        TransactionError::from(Error::new(ErrorCode::InvalidInput, "generation"))
                    })?,
                    schema_epoch: SchemaEpoch::new(1).map_err(|_| {
                        TransactionError::from(Error::new(ErrorCode::InvalidInput, "schema epoch"))
                    })?,
                };
                let schema = Schema::new(spec, &self.shared.resident)?;
                let table = TableRoot::empty(identity, schema, &self.shared.resident)?;
                let next = current.edited(None, Some(table), &self.shared.resident)?;
                Ok((
                    next,
                    TableHandle {
                        instance: Arc::clone(&self.shared.instance),
                        identity,
                    },
                ))
            },
            before_publish,
        )
    }

    pub(crate) fn drop_table_with<E>(
        &self,
        table: &TableHandle,
        before_publish: impl FnOnce(&PreparedCommit) -> std::result::Result<(), HookFailure<E>>,
    ) -> std::result::Result<(), CommitWithError<E>> {
        self.ddl_with(
            |current, _| {
                self.shared.counters.operation();
                let index = table_index(&self.shared.instance, current, table)?;
                Ok((
                    current.edited(Some(index), None, &self.shared.resident)?,
                    (),
                ))
            },
            before_publish,
        )
    }

    fn ddl_with<T, E>(
        &self,
        prepare: impl FnOnce(&Root, &mut Coordinator) -> TransactionResult<(Arc<Root>, T)>,
        hook: impl FnOnce(&PreparedCommit) -> std::result::Result<(), HookFailure<E>>,
    ) -> std::result::Result<T, CommitWithError<E>> {
        let (_admission, _pin) = self.shared.admission()?;
        let mut coordinator = self.shared.coordinator()?;
        self.shared.open()?;
        let id = coordinator.transaction_id(self.shared.instance.namespace)?;
        let current = self.shared.snapshot().map_err(|error| error.id(id))?;
        let (root, output) =
            prepare(&current.root, &mut coordinator).map_err(|error| error.id(id).aborted())?;
        let position = next_position(current.position).map_err(|error| error.id(id))?;
        let prepared = CommittedRoot::prepare(position, Arc::clone(&root), &self.shared.resident)
            .map_err(|error| error.id(id).aborted())?;
        let change = PreparedCommit {
            id,
            position,
            before: Arc::clone(&current.root),
            after: root,
            cancellation: Cancellation::default(),
        };
        invoke_hook(&self.shared, &change, hook)?;
        let old = self.shared.publication.replace(prepared).map_err(|error| {
            self.shared.fence();
            CommitWithError::Publication(error.id(id).unknown())
        })?;
        drop(coordinator);
        drop(old);
        drop(current);
        Ok(output)
    }

    /// Observe work and resource gauges. Runtime quota handles and caller-owned
    /// input/output allocation overhead are outside these logical byte ledgers.
    pub fn metrics(&self) -> TransactionMetrics {
        let counters = &self.shared.transaction_counters;
        let (oldest_user_pin_age_ns, user_pins_over_sixty_seconds) = counters.pin_ages();
        let (wait_ns, hold_ns, clock_invalid, saturated) = counters.timing();
        TransactionMetrics {
            substrate: self
                .shared
                .counters
                .snapshot(&self.shared.resident, &self.shared.pins),
            private_bytes: self.shared.private_pool.used(),
            active_transactions: self.shared.admitted.used(),
            oldest_user_pin_age_ns,
            user_pins_over_sixty_seconds,
            coordinator_acquisitions: counters.acquisitions.load(Ordering::Relaxed),
            coordinator_wait_ns: wait_ns,
            coordinator_hold_ns: hold_ns,
            coordinator_clock_invalid: clock_invalid,
            counters_saturated: saturated,
            serialization_conflicts: counters.conflicts.load(Ordering::Relaxed),
            statement_rollbacks: counters.statement_rollbacks.load(Ordering::Relaxed),
            commit_preparation_failures: counters.preparation_failures.load(Ordering::Relaxed),
        }
    }

    /// Whether this volatile core refuses further admission and mutation.
    pub fn is_fenced(&self) -> bool {
        self.shared.fenced.load(Ordering::Acquire)
    }

    pub(crate) fn committed_root(&self) -> TransactionResult<Arc<CommittedRoot>> {
        self.shared.snapshot()
    }

    /// Capture a root and a durable owner's anchor at one coordinator boundary.
    /// The closure runs with no publication lock and must not reacquire coordinator.
    pub(crate) fn with_committed<T, E>(
        &self,
        capture: impl FnOnce(&CommittedRoot) -> std::result::Result<T, E>,
    ) -> std::result::Result<T, CommitWithError<E>> {
        let coordinator = self.shared.coordinator()?;
        self.shared.open()?;
        let current = self.shared.snapshot()?;
        let result = capture(&current).map_err(CommitWithError::Hook);
        drop(coordinator);
        drop(current);
        result
    }

    pub(crate) fn fence(&self) {
        self.shared.fence();
    }

    pub(crate) fn resident_quota(&self) -> Quota {
        self.shared.resident.clone()
    }
}

fn next_position(current: u64) -> TransactionResult<u64> {
    current.checked_add(1).ok_or_else(|| {
        TransactionError::from(Error::new(
            ErrorCode::IdentityExhausted,
            "commit position exhausted",
        ))
        .aborted()
    })
}

fn invoke_hook<E>(
    shared: &Shared,
    prepared: &PreparedCommit,
    hook: impl FnOnce(&PreparedCommit) -> std::result::Result<(), HookFailure<E>>,
) -> std::result::Result<(), CommitWithError<E>> {
    if let Err(failure) = hook(prepared) {
        if failure.fence {
            shared.fence();
        }
        return Err(CommitWithError::Hook(failure.error));
    }
    Ok(())
}

struct Active {
    root: Arc<Root>,
    start_position: u64,
    private: AllocationBudget,
    successful_mutations: usize,
    dirty: bool,
    _admission: Reservation,
    _pin: UserPin,
}

/// One private transaction. Mutation requires exclusive access. Borrowed row/page
/// results prevent mutation, commit or rollback while their referenced root lives.
/// Drop only releases private state, admission and pin; it never invokes a hook.
pub struct Transaction {
    shared: Arc<Shared>,
    id: TxnId,
    mode: TransactionMode,
    cancellation: Cancellation,
    active: Option<Active>,
}

impl Transaction {
    /// Identity fixed at begin; remains available after cooperative abort.
    pub const fn id(&self) -> TxnId {
        self.id
    }
    /// Cancellation is observed before statements and before the commit hook.
    /// Once the hook starts it owns the durable decision and cancellation is ignored.
    pub fn cancellation(&self) -> Cancellation {
        self.cancellation.clone()
    }
    /// Current private allocation leases; zero after cooperative abort.
    pub fn private_bytes(&self) -> u64 {
        self.active
            .as_ref()
            .map_or(0, |active| active.private.used())
    }

    fn active(&self) -> TransactionResult<&Active> {
        self.active.as_ref().ok_or_else(|| {
            TransactionError::new(TransactionErrorCode::Closed, "transaction is aborted")
                .id(self.id)
                .aborted()
        })
    }
    fn mutation_boundary(&mut self) -> TransactionResult<()> {
        self.active()?;
        let result = self.shared.open().and_then(|()| {
            self.cancellation.check().map_err(|_| {
                TransactionError::new(
                    TransactionErrorCode::Cancelled,
                    "cancelled before commit attempt",
                )
                .aborted()
            })
        });
        if let Err(error) = result {
            self.active.take();
            return Err(error.id(self.id).aborted());
        }
        Ok(())
    }

    /// Apply the entire input sequence to a staged private root, then install it.
    /// The cumulative 256-mutation bound counts only successful statement inputs.
    /// Failed statements consume already issued IDs but restore root/quota exactly.
    ///
    /// # Errors
    /// Input/schema/missing-row/quota errors preserve earlier private statements.
    /// Cancellation, fencing and poison abort the whole transaction. Result buffer
    /// allocation and rollback capacity are prepared before any mutation is staged.
    pub fn execute_statement(
        &mut self,
        mutations: Vec<Mutation>,
    ) -> TransactionResult<StatementResult> {
        self.mutation_boundary()?;
        if self.mode == TransactionMode::ReadOnly {
            return Err(TransactionError::new(
                TransactionErrorCode::ReadOnly,
                "read-only statement",
            )
            .id(self.id));
        }
        let count = mutations.len();
        let result = (|| {
            let active = self.active()?;
            if count > 256 || active.successful_mutations + count > 256 {
                return Err(Error::new(
                    ErrorCode::LimitExceeded,
                    "256 successful transaction mutations",
                )
                .into());
            }
            let (mut inserted, output_charge) = array_in::<RowUid>(&active.private, count, 0)?;
            let mut staged = Arc::clone(&active.root);
            for mutation in mutations {
                self.shared.open()?;
                self.cancellation.check().map_err(|_| {
                    TransactionError::new(TransactionErrorCode::Cancelled, "statement cancelled")
                })?;
                self.shared.counters.operation();
                let (table, row, replacement, insert) = match mutation {
                    Mutation::Insert { table, values } => {
                        let index = table_index(&self.shared.instance, &staged, &table)?;
                        let bytes =
                            catalog::validate_row(&staged.tables[index].schema.spec, &values)?;
                        let row = {
                            let mut coordinator = self.shared.coordinator()?;
                            self.shared.open()?;
                            coordinator.issuer.row()?
                        };
                        let uid = RowUid {
                            namespace: self.shared.instance.namespace,
                            table: table.identity,
                            row,
                        };
                        let stored = Arc::new(StoredRow::new_in(values, bytes, &active.private)?);
                        inserted.push(uid);
                        (table, uid, Some(stored), true)
                    }
                    Mutation::Replace { table, row, values } => {
                        let index = table_index(&self.shared.instance, &staged, &table)?;
                        check_row(self.shared.instance.namespace, table.identity, row)?;
                        if staged.tables[index].find(row.row).is_err() {
                            return Err(
                                Error::new(ErrorCode::RowNotFound, "replace missing row").into()
                            );
                        }
                        let bytes =
                            catalog::validate_row(&staged.tables[index].schema.spec, &values)?;
                        let stored = Arc::new(StoredRow::new_in(values, bytes, &active.private)?);
                        (table, row, Some(stored), false)
                    }
                    Mutation::Delete { table, row } => (table, row, None, false),
                };
                let index = table_index(&self.shared.instance, &staged, &table)?;
                check_row(self.shared.instance.namespace, table.identity, row)?;
                let changed = staged.tables[index].edited_in(
                    row.row,
                    replacement,
                    insert,
                    &active.private,
                    &self.shared.counters,
                )?;
                staged = staged.edited_in(Some(index), Some(changed), &active.private)?;
            }
            self.shared.open()?;
            self.cancellation.check().map_err(|_| {
                TransactionError::new(
                    TransactionErrorCode::Cancelled,
                    "statement cancelled before install",
                )
            })?;
            // No reader borrows self across this exclusive operation. The old
            // private root is released only after all fallible preparation ended.
            let active = self
                .active
                .as_mut()
                .expect("active state unchanged during statement");
            active.root = staged;
            active.successful_mutations += count;
            active.dirty |= count != 0;
            drop(output_charge);
            Ok(StatementResult {
                inserted,
                affected_rows: count as u64,
            })
        })();
        result.map_err(|error: TransactionError| {
            let counters = &self.shared.transaction_counters;
            counters.add(&counters.statement_rollbacks, 1);
            let error = error.id(self.id);
            if matches!(
                error.code,
                TransactionErrorCode::Cancelled
                    | TransactionErrorCode::Fenced
                    | TransactionErrorCode::Poisoned
            ) {
                self.active.take();
                error.aborted()
            } else {
                error
            }
        })
    }

    /// Finish a volatile transaction; this is never a durable acknowledgement.
    ///
    /// # Errors
    /// Conflict, cancellation or preparation failure consumes/aborts this whole
    /// transaction. Poison fences the core; no retry reuses private state.
    pub fn commit(self) -> TransactionResult<VolatileCommit> {
        volatile_result(self.commit_with(|_| Ok(())))
    }

    pub(crate) fn commit_with<E>(
        mut self,
        before_publish: impl FnOnce(&PreparedCommit) -> std::result::Result<(), HookFailure<E>>,
    ) -> std::result::Result<VolatileCommit, CommitWithError<E>> {
        self.mutation_boundary()?;
        let shared = Arc::clone(&self.shared);
        let coordinator = shared.coordinator().map_err(|error| error.id(self.id))?;
        shared.open().map_err(|error| error.id(self.id))?;
        let active = self.active()?;
        if self.mode == TransactionMode::ReadOnly {
            self.cancellation.check().map_err(|_| {
                TransactionError::new(
                    TransactionErrorCode::Cancelled,
                    "cancelled before completion",
                )
                .id(self.id)
                .aborted()
            })?;
            return Ok(VolatileCommit {
                transaction: self.id,
                position: active.start_position,
                published: false,
            });
        }
        let current = shared.snapshot().map_err(|error| error.id(self.id))?;
        if current.position != active.start_position {
            shared
                .transaction_counters
                .add(&shared.transaction_counters.conflicts, 1);
            let mut error = TransactionError::new(
                TransactionErrorCode::SerializationConflict,
                "stale writer snapshot",
            )
            .id(self.id)
            .aborted();
            error.retry = TransactionRetry::Transaction;
            return Err(error.into());
        }
        if !active.dirty || active.root.logically_equal(&current.root) {
            self.cancellation.check().map_err(|_| {
                TransactionError::new(
                    TransactionErrorCode::Cancelled,
                    "cancelled before completion",
                )
                .id(self.id)
                .aborted()
            })?;
            return Ok(VolatileCommit {
                transaction: self.id,
                position: current.position,
                published: false,
            });
        }
        let (prepared, change, receipt) = (|| {
            let root = active
                .root
                .resident(&shared.resident, &shared.counters)
                .map_err(|error| TransactionError::from(error).id(self.id).aborted())?;
            let position = next_position(current.position).map_err(|error| error.id(self.id))?;
            let prepared = CommittedRoot::prepare(position, Arc::clone(&root), &shared.resident)
                .map_err(|error| error.id(self.id).aborted())?;
            let change = PreparedCommit {
                id: self.id,
                position,
                before: Arc::clone(&current.root),
                after: root,
                cancellation: self.cancellation.clone(),
            };
            let receipt = VolatileCommit {
                transaction: self.id,
                position,
                published: true,
            };
            self.cancellation.check().map_err(|_| {
                TransactionError::new(
                    TransactionErrorCode::Cancelled,
                    "cancelled before commit hook",
                )
                .id(self.id)
                .aborted()
            })?;
            Ok((prepared, change, receipt))
        })()
        .inspect_err(|_: &TransactionError| {
            shared
                .transaction_counters
                .add(&shared.transaction_counters.preparation_failures, 1);
        })?;
        invoke_hook(&shared, &change, before_publish)?;
        let old = shared.publication.replace(prepared).map_err(|error| {
            shared.fence();
            CommitWithError::Publication(error.id(self.id).unknown())
        })?;
        drop(coordinator);
        drop(old);
        drop(current);
        self.active.take();
        Ok(receipt)
    }

    /// Consume the transaction and release all private state/pins without storage
    /// effects. Rollback remains available after fencing or cooperative abort.
    pub fn rollback(mut self) {
        self.active.take();
    }
}
