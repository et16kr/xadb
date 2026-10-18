//! Native durable API adapter. Each actor owns its transaction on a real thread.
//! The controller owns deadlines and evidence. Fatal transport errors exit without
//! inventing aborted observations for effects another actor may already have made.
use super::fault;
use super::{Aliases, ProtocolError, from_spec, row_identity, table_identity, to_spec, to_values};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Read, Write},
    path::PathBuf,
    sync::{Arc, Barrier, Mutex, MutexGuard, mpsc},
    thread,
};
use xadb_engine::{
    api::{RowUid, TableHandle},
    database::{DurableTransaction, Engine, EngineError, EngineOptions, TransactionOutcome},
    transaction::{
        Mutation, TransactionEffect, TransactionError, TransactionErrorCode, TransactionMode,
        TransactionRetry, TransactionRow, TxnId,
    },
};
use xadb_foundation::{error::Certainty, value::Value};
use xadb_platform::{namespace::LinuxDirectory, runtime::MonotonicClock};
use xadbtest_protocol::{durable as wire, frame};

const RUN_BYTES: usize = 64 * 1024 * 1024;
const MAX_OBSERVED_ROWS: usize = 32768;
type Database = Engine<fault::Writer>;
type Transaction = DurableTransaction<fault::Writer>;
type Result<T> = std::result::Result<T, ProtocolError>;
type RowKey = ([u8; 16], u64, u64, u64, u64, u64);

fn key(identity: &wire::RowIdentity) -> RowKey {
    (
        identity.table.database,
        identity.table.timeline,
        identity.table.object,
        identity.table.generation,
        identity.table.schema_epoch,
        identity.row,
    )
}
fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>> {
    mutex
        .lock()
        .map_err(|_| ProtocolError("worker state poisoned"))
}
struct Gate {
    actors: Vec<u8>,
    dispatched: u32,
    barrier: Arc<Barrier>,
}
struct Shared {
    database: Option<Database>,
    faults: Arc<fault::Hub>,
    observations: bool,
    clock: bool,
    tables: Aliases<TableHandle>,
    rows: Aliases<RowUid>,
    observed: BTreeMap<RowKey, RowUid>,
    gates: BTreeMap<u64, Gate>,
    busy: [bool; wire::MAX_PENDING],
    active: usize,
    output_bytes: usize,
}
impl Shared {
    fn database(&self) -> Result<Database> {
        self.database
            .clone()
            .ok_or(ProtocolError("no open database"))
    }
    fn observe(&mut self, uid: RowUid) -> Result<()> {
        let key = key(&row_identity(uid));
        if !self.observed.contains_key(&key) && self.observed.len() == MAX_OBSERVED_ROWS {
            return Err(ProtocolError("observed row capability bound"));
        }
        self.observed.insert(key, uid);
        Ok(())
    }
}
struct Work {
    request: wire::Request,
    gate: Option<Arc<Barrier>>,
}
struct Actor {
    transaction: Option<(u64, Transaction)>,
    transactions: BTreeSet<u64>,
    cursors: BTreeSet<u64>,
}
impl Actor {
    fn transaction(&self, alias: u64) -> Result<&Transaction> {
        self.transaction
            .as_ref()
            .filter(|(id, _)| *id == alias)
            .map(|(_, t)| t)
            .ok_or(ProtocolError("actor transaction alias absent"))
    }
    fn transaction_mut(&mut self, alias: u64) -> Result<&mut Transaction> {
        self.transaction
            .as_mut()
            .filter(|(id, _)| *id == alias)
            .map(|(_, t)| t)
            .ok_or(ProtocolError("actor transaction alias absent"))
    }
}
pub(crate) fn identity(id: TxnId) -> wire::TransactionIdentity {
    wire::TransactionIdentity {
        database: id.database.bytes(),
        timeline: id.timeline,
        writer_epoch: id.writer_epoch,
        local: id.local,
    }
}
fn transaction_error(error: TransactionError) -> wire::Observation {
    wire::Observation::Error {
        failure: wire::Failure::Transaction {
            code: match error.code {
                TransactionErrorCode::Product(code) => code.id().into(),
                other => format!("{other:?}"),
            },
            effect: match error.effect {
                TransactionEffect::Active => wire::Effect::Active,
                TransactionEffect::Aborted => wire::Effect::Aborted,
                TransactionEffect::Unknown => wire::Effect::Unknown,
            },
            certainty: match error.certainty {
                Certainty::Known => wire::Certainty::Known,
                Certainty::Unknown => wire::Certainty::Unknown,
            },
            retry: match error.retry {
                TransactionRetry::Never => wire::Retry::Never,
                TransactionRetry::Transaction => wire::Retry::Transaction,
                TransactionRetry::AfterRecovery => wire::Retry::AfterRecovery,
            },
            transaction: error.transaction.map(identity),
        },
    }
}
pub(crate) fn engine_error(error: EngineError) -> wire::Observation {
    match error {
        EngineError::Transaction(error) => transaction_error(error),
        EngineError::Storage { error, transaction } => wire::Observation::Error {
            failure: wire::Failure::Storage {
                code: format!("{:?}", error.code),
                outcome: match error.outcome {
                    TransactionOutcome::NotAttempted => wire::Outcome::NotAttempted,
                    TransactionOutcome::Aborted => wire::Outcome::Aborted,
                    TransactionOutcome::Unknown => wire::Outcome::Unknown,
                    TransactionOutcome::Committed => wire::Outcome::Committed,
                },
                fenced: error.fenced,
                transaction: transaction.map(identity),
            },
        },
    }
}
fn row(view: TransactionRow<'_>) -> wire::Row {
    wire::Row {
        identity: row_identity(view.uid()),
        values: (0..view.len())
            .map(
                |column| match view.value(column).expect("column within view") {
                    Value::Null => wire::Cell::Null,
                    Value::Bool(v) => wire::Cell::Bool(*v),
                    Value::Int64(v) => wire::Cell::Int64(*v),
                    Value::Utf8(v) => wire::Cell::Utf8(v.clone()),
                    Value::Bytes(v) => wire::Cell::Binary(v.clone()),
                },
            )
            .collect(),
    }
}
fn directory(slot: u8, create: bool, hub: Arc<fault::Hub>) -> Result<fault::Directory> {
    let path = PathBuf::from(format!("database-{slot}"));
    if create {
        std::fs::create_dir(&path).map_err(|_| ProtocolError("fixture directory create failed"))?;
    }
    LinuxDirectory::open_existing(&path)
        .map(|port| fault::Directory::new(port, hub, slot))
        .map_err(|_| ProtocolError("fixture directory open failed"))
}
fn options(state: &Shared) -> EngineOptions {
    EngineOptions {
        observation_enabled: state.observations,
        clock: state.clock.then(|| {
            Arc::new(MonotonicClock::default())
                as Arc<dyn xadb_foundation::runtime::Clock + Send + Sync>
        }),
        ..EngineOptions::default()
    }
}
// Reserve a conservative serialized response bound before an operation can run.
// Full-row/table/page replies use the framing maximum; commit/readiness replies
// are fixed-size and allow all 32 actors to rendezvous within the run budget.
fn output_reservation(action: &wire::Action) -> usize {
    match action {
        wire::Action::CreateTable { .. }
        | wire::Action::Lookup { .. }
        | wire::Action::Read { .. }
        | wire::Action::NextPage { .. } => frame::MAX_FRAME_BYTES,
        wire::Action::Statement { .. } => 256 * 1024,
        wire::Action::DurabilityMetrics { .. } => 256 * 1024,
        _ => 16 * 1024,
    }
}
fn complete(
    shared: &Mutex<Shared>,
    request: wire::Request,
    observation: wire::Observation,
) -> Result<()> {
    let reserved = output_reservation(&request.command.action);
    let response = wire::Response {
        version: wire::VERSION,
        run: request.run,
        request: request.request,
        actor: request.actor,
        passed_gate: request.command.gate,
        observation,
    };
    let bytes = frame::encode(&response)
        .map_err(|_| ProtocolError("response encoding failed; effect remains unobserved"))?;
    if bytes.len() > reserved {
        return Err(ProtocolError(
            "response exceeded pre-effect bound; effect unobserved",
        ));
    }
    let mut state = lock(shared)?;
    let mut output = io::stdout().lock();
    output
        .write_all(&bytes)
        .and_then(|()| output.flush())
        .map_err(|_| ProtocolError("response write failed; effect remains unobserved"))?;
    state.output_bytes -= reserved - bytes.len();
    state.busy[usize::from(request.actor)] = false;
    Ok(())
}
fn exclusive(
    shared: &Mutex<Shared>,
    action: &wire::Action,
    run: [u8; 16],
) -> Result<wire::Observation> {
    use wire::{Action as A, Observation as O};
    let mut state = lock(shared)?;
    if !matches!(action, A::ConfigureGate { .. } | A::ConfigureFault { .. }) && state.active != 0 {
        return Err(ProtocolError(
            "close/open/restore with active transaction or cursor",
        ));
    }
    match action {
        A::ConfigureFault { token, plan } => {
            state
                .faults
                .configure(run, *token, *plan)
                .map_err(|_| ProtocolError("fault control configure failed"))?;
            Ok(O::Done {})
        }
        A::ConfigureEngine {
            observations,
            clock,
        } => {
            if state.database.is_some() {
                return Err(ProtocolError("configure engine requires closed database"));
            }
            state.observations = *observations;
            state.clock = *clock;
            Ok(O::Done {})
        }
        A::ConfigureGate { gate, actors } => {
            if state.gates.contains_key(gate) {
                return Err(ProtocolError("gate id already consumed"));
            }
            state.gates.insert(
                *gate,
                Gate {
                    actors: actors.clone(),
                    dispatched: 0,
                    barrier: Arc::new(Barrier::new(actors.len())),
                },
            );
            Ok(O::Done {})
        }
        A::CloseDatabase {} => {
            state
                .database
                .take()
                .ok_or(ProtocolError("no open database"))?;
            Ok(O::Done {})
        }
        A::CreateDatabase { directory: slot } | A::OpenDatabase { directory: slot } => {
            if state.database.is_some() {
                return Err(ProtocolError("database already open"));
            }
            let create = matches!(action, A::CreateDatabase { .. });
            let port = directory(*slot, create, Arc::clone(&state.faults))?;
            let result = if create {
                Database::create(&port, options(&state))
            } else {
                Database::open(&port, options(&state))
            };
            match result {
                Ok(database) => {
                    let observation = O::Ready {
                        database: database.database_id().bytes(),
                        timeline: database.timeline(),
                    };
                    state.database = Some(database);
                    Ok(observation)
                }
                Err(error) => Ok(engine_error(error)),
            }
        }
        A::RestoreDatabase { source, target } => {
            if state.database.is_some() {
                return Err(ProtocolError("restore requires closed database"));
            }
            let source = directory(*source, false, Arc::clone(&state.faults))?;
            let target = directory(*target, true, Arc::clone(&state.faults))?;
            match Database::restore_new(&source, &target, options(&state)) {
                Ok(database) => {
                    let observation = O::Ready {
                        database: database.database_id().bytes(),
                        timeline: database.timeline(),
                    };
                    state.database = Some(database);
                    Ok(observation)
                }
                Err(error) => Ok(engine_error(error)),
            }
        }
        _ => Err(ProtocolError("not an exclusive operation")),
    }
}
fn operation(
    actor: &mut Actor,
    shared: &Mutex<Shared>,
    action: &wire::Action,
) -> Result<wire::Observation> {
    use wire::{Action as A, Observation as O};
    match action {
        A::Begin { transaction, mode } => {
            if actor.transaction.is_some() || !actor.transactions.insert(*transaction) {
                return Err(ProtocolError("active or reused transaction alias"));
            }
            let database = lock(shared)?.database()?;
            match database.begin(match mode {
                wire::Mode::ReadOnly => TransactionMode::ReadOnly,
                wire::Mode::ReadWrite => TransactionMode::ReadWrite,
            }) {
                Ok(value) => {
                    let id = value.id();
                    actor.transaction = Some((*transaction, value));
                    lock(shared)?.active += 1;
                    Ok(O::Begun {
                        identity: identity(id),
                    })
                }
                Err(error) => Ok(engine_error(error)),
            }
        }
        A::CreateTable { table, spec } => {
            if actor.transaction.is_some() {
                return Err(ProtocolError("DDL within actor transaction"));
            }
            let database = {
                let mut state = lock(shared)?;
                state.tables.reserve(*table)?;
                state.database()?
            };
            match database.create_table_with_receipt(to_spec(spec)) {
                Ok((handle, receipt)) => {
                    let id = table_identity(
                        &handle,
                        database.database_id().bytes(),
                        database.timeline(),
                    );
                    lock(shared)?.tables.install(*table, handle);
                    Ok(O::Table {
                        identity: id,
                        spec: spec.clone(),
                        commit: Some(wire::CommitIdentity {
                            transaction: identity(receipt.transaction),
                            position: receipt.position,
                            wrote: receipt.wrote,
                        }),
                    })
                }
                Err(error) => Ok(engine_error(error)),
            }
        }
        A::DropTable { table } => {
            if actor.transaction.is_some() {
                return Err(ProtocolError("DDL within actor transaction"));
            }
            let (database, handle) = {
                let state = lock(shared)?;
                (state.database()?, state.tables.get(*table)?.clone())
            };
            Ok(match database.drop_table_with_receipt(&handle) {
                Ok(receipt) => O::Committed {
                    transaction: identity(receipt.transaction),
                    position: receipt.position,
                    wrote: receipt.wrote,
                },
                Err(error) => engine_error(error),
            })
        }
        A::Lookup {
            transaction,
            table,
            name,
        } => {
            let txn = actor.transaction(*transaction)?;
            lock(shared)?.tables.reserve(*table)?;
            match txn.lookup_table(name) {
                Ok(handle) => {
                    let spec = match txn.schema(&handle) {
                        Ok(spec) => from_spec(spec),
                        Err(error) => return Ok(transaction_error(error)),
                    };
                    let id = table_identity(&handle, txn.id().database.bytes(), txn.id().timeline);
                    lock(shared)?.tables.install(*table, handle);
                    Ok(O::Table {
                        identity: id,
                        spec,
                        commit: None,
                    })
                }
                Err(error) => Ok(transaction_error(error)),
            }
        }
        A::BindRow { row, identity } => {
            let mut state = lock(shared)?;
            state.rows.reserve(*row)?;
            let uid = *state
                .observed
                .get(&key(identity))
                .ok_or(ProtocolError("row identity has not been observed"))?;
            state.rows.install(*row, uid);
            Ok(O::Done {})
        }
        A::Statement {
            transaction,
            mutations,
        } => {
            let txn = actor.transaction_mut(*transaction)?;
            let mut values = Vec::with_capacity(mutations.len());
            let mut inserted = Vec::new();
            {
                let mut state = lock(shared)?;
                for mutation in mutations {
                    values.push(match mutation {
                        wire::Mutation::Insert { table, row, values } => {
                            let handle = state.tables.get(*table)?.clone();
                            state.rows.reserve(*row)?;
                            inserted.push(*row);
                            Mutation::Insert {
                                table: handle,
                                values: to_values(values),
                            }
                        }
                        wire::Mutation::Replace { table, row, values } => Mutation::Replace {
                            table: state.tables.get(*table)?.clone(),
                            row: *state.rows.get(*row)?,
                            values: to_values(values),
                        },
                        wire::Mutation::Delete { table, row } => Mutation::Delete {
                            table: state.tables.get(*table)?.clone(),
                            row: *state.rows.get(*row)?,
                        },
                    });
                }
            }
            match txn.execute_statement(values) {
                Ok(result) => {
                    if result.inserted.len() != inserted.len() {
                        return Err(ProtocolError("product insert result shape"));
                    }
                    let mut state = lock(shared)?;
                    let mut issued = Vec::with_capacity(inserted.len());
                    for (alias, uid) in inserted.into_iter().zip(result.inserted) {
                        state.rows.install(alias, uid);
                        issued.push(wire::IssuedRow {
                            alias,
                            identity: row_identity(uid),
                        });
                    }
                    Ok(O::Statement {
                        inserted: issued,
                        affected_rows: result.affected_rows,
                    })
                }
                Err(error) => Ok(transaction_error(error)),
            }
        }
        A::Read {
            transaction,
            table,
            row: alias,
        } => {
            let txn = actor.transaction(*transaction)?;
            let (handle, uid) = {
                let state = lock(shared)?;
                (state.tables.get(*table)?.clone(), *state.rows.get(*alias)?)
            };
            match txn.read(&handle, uid) {
                Ok(view) => {
                    if let Some(view) = view {
                        lock(shared)?.observe(view.uid())?;
                    }
                    Ok(O::Row {
                        value: view.map(row),
                    })
                }
                Err(error) => Ok(transaction_error(error)),
            }
        }
        A::Commit { transaction } | A::Rollback { transaction } => {
            actor.transaction(*transaction)?;
            let (_, txn) = actor.transaction.take().expect("checked transaction");
            let observation = if matches!(action, A::Commit { .. }) {
                match txn.commit() {
                    Ok(receipt) => O::Committed {
                        transaction: identity(receipt.transaction),
                        position: receipt.position,
                        wrote: receipt.wrote,
                    },
                    Err(error) => engine_error(error),
                }
            } else {
                txn.rollback();
                O::Done {}
            };
            lock(shared)?.active -= 1;
            Ok(observation)
        }
        A::Cancel { transaction } => {
            actor.transaction(*transaction)?.cancellation().cancel();
            Ok(O::Done {})
        }
        A::Checkpoint {} => {
            let database = lock(shared)?.database()?;
            Ok(match database.checkpoint() {
                Ok(receipt) => O::Checkpoint {
                    generation: receipt.generation,
                    position: receipt.position,
                },
                Err(error) => engine_error(error),
            })
        }
        A::CollectGarbage {} => {
            let database = lock(shared)?.database()?;
            Ok(match database.collect_garbage() {
                Ok(receipt) => O::GarbageCollected {
                    removed: receipt.files as u64,
                },
                Err(error) => engine_error(error),
            })
        }
        A::Backup { directory: slot } => {
            let database = lock(shared)?.database()?;
            let target = directory(*slot, true, Arc::clone(&lock(shared)?.faults))?;
            Ok(match database.backup(&target) {
                Ok(receipt) => O::Backup {
                    generation: receipt.generation,
                    position: receipt.position,
                },
                Err(error) => engine_error(error),
            })
        }
        A::DurabilityMetrics {} => {
            let database = lock(shared)?.database()?;
            Ok(match database.durability_metrics() {
                Ok(metrics) => O::DurabilityMetrics {
                    counters: durability_metrics(metrics),
                },
                Err(error) => engine_error(error),
            })
        }
        A::Metrics {} => {
            let metrics = lock(shared)?.database()?.metrics();
            Ok(O::Metrics {
                counters: wire::Metrics {
                    operations: metrics.substrate.operations,
                    rows_read: metrics.substrate.rows_read,
                    row_references_copied: metrics.substrate.row_references_copied,
                    resident_bytes: metrics.substrate.resident_bytes,
                    snapshot_pins: metrics.substrate.snapshot_pins,
                    private_bytes: metrics.private_bytes,
                    active_transactions: metrics.active_transactions,
                    oldest_user_pin_age_ns: metrics.oldest_user_pin_age_ns,
                    user_pins_over_sixty_seconds: metrics.user_pins_over_sixty_seconds,
                    coordinator_acquisitions: metrics.coordinator_acquisitions,
                    coordinator_wait_ns: metrics.coordinator_wait_ns,
                    coordinator_hold_ns: metrics.coordinator_hold_ns,
                    coordinator_clock_invalid: metrics.coordinator_clock_invalid,
                    counters_saturated: metrics.counters_saturated,
                    serialization_conflicts: metrics.serialization_conflicts,
                    statement_rollbacks: metrics.statement_rollbacks,
                    commit_preparation_failures: metrics.commit_preparation_failures,
                },
            })
        }
        _ => Err(ProtocolError("operation invalid in actor state")),
    }
}

pub(crate) fn durability_metrics(
    metrics: xadb_engine::database::DurabilityMetrics,
) -> wire::DurabilityMetrics {
    let gauges = metrics.gauges;
    wire::DurabilityMetrics {
        semantic_revision: metrics.semantic_revision,
        operations: metrics.operations.map(|operations| {
            operations
                .into_iter()
                .map(|metric| wire::DurabilityOperationMetrics {
                    metric_id: metric.operation.metric_id().into(),
                    attempts: metric.attempts,
                    successes: metric.successes,
                    failures: metric.failures,
                    requested_bytes: metric.requested_bytes,
                    confirmed_complete_bytes: metric.confirmed_complete_bytes,
                    items: metric.items,
                    unknown_partial_operations: metric.unknown_partial_operations,
                    timing_missing: metric.timing_missing,
                    elapsed: metric.elapsed.map(|timing| wire::DurabilityTiming {
                        samples: timing.samples,
                        sum_ns: timing.sum_ns,
                        max_ns: timing.max_ns,
                        log2_ns: timing.log2_ns.to_vec(),
                    }),
                })
                .collect()
        }),
        clock_configured: metrics.clock_configured,
        clock_invalid: metrics.clock_invalid,
        counters_saturated: metrics.counters_saturated,
        gauges: wire::DurabilityGauges {
            retained_wal_bytes: gauges.retained_wal_bytes,
            durable_position: gauges.durable_position,
            checkpoint_position: gauges.checkpoint_position,
            control_reserved_bytes: gauges.control_reserved_bytes,
            control_used_bytes: gauges.control_used_bytes,
            oldest_system_pin_age_ns: gauges.oldest_system_pin_age_ns,
            system_pins_over_60_seconds: gauges.system_pins_over_60_seconds,
        },
    }
}

fn pass(work: &Work) {
    if let Some(gate) = &work.gate {
        gate.wait();
    }
}
fn actor_loop(shared: Arc<Mutex<Shared>>, receiver: mpsc::Receiver<Work>) -> Result<()> {
    let mut actor = Actor {
        transaction: None,
        transactions: BTreeSet::new(),
        cursors: BTreeSet::new(),
    };
    while let Ok(work) = receiver.recv() {
        pass(&work);
        let _scope = fault::Scope::enter(&work.request);
        if let wire::Action::OpenScan {
            transaction,
            table,
            cursor,
        } = work.request.command.action.clone()
        {
            if !actor.cursors.insert(cursor) {
                return Err(ProtocolError("cursor alias consumed"));
            }
            let txn = actor.transaction(transaction)?;
            let handle = lock(&shared)?.tables.get(table)?.clone();
            let mut scan = match txn.scan(&handle) {
                Ok(scan) => scan,
                Err(error) => {
                    complete(&shared, work.request, transaction_error(error))?;
                    continue;
                }
            };
            complete(&shared, work.request, wire::Observation::Done {})?;
            loop {
                let next = receiver
                    .recv()
                    .map_err(|_| ProtocolError("actor closed with active cursor"))?;
                pass(&next);
                let _scope = fault::Scope::enter(&next.request);
                let observation = match next.request.command.action {
                    wire::Action::NextPage {
                        cursor: alias,
                        limit,
                    } if alias == cursor => {
                        if lock(&shared)?.observed.len() + limit as usize > MAX_OBSERVED_ROWS {
                            return Err(ProtocolError("scan observation capacity"));
                        }
                        match scan.next_page(limit as usize) {
                            Ok(page) => {
                                let mut state = lock(&shared)?;
                                for view in page.rows() {
                                    state.observe(view.uid())?;
                                }
                                wire::Observation::Page {
                                    rows: page.rows().iter().copied().map(row).collect(),
                                    eof: page.eof(),
                                    canonical_bytes: page.canonical_bytes() as u32,
                                }
                            }
                            Err(error) => transaction_error(error),
                        }
                    }
                    wire::Action::CloseScan { cursor: alias } if alias == cursor => {
                        complete(&shared, next.request, wire::Observation::Done {})?;
                        break;
                    }
                    _ => {
                        return Err(ProtocolError(
                            "active cursor permits only next_page/close_scan",
                        ));
                    }
                };
                complete(&shared, next.request, observation)?;
            }
        } else {
            let observation = operation(&mut actor, &shared, &work.request.command.action)?;
            complete(&shared, work.request, observation)?;
        }
    }
    if actor.transaction.is_some() {
        return Err(ProtocolError("actor closed with active transaction"));
    }
    Ok(())
}
struct Input<R> {
    reader: R,
    remaining: usize,
}
impl<R: Read> Read for Input<R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let count = bytes.len().min(self.remaining.saturating_add(1));
        let count = self.reader.read(&mut bytes[..count])?;
        if count > self.remaining {
            return Err(io::Error::other("durable run input limit"));
        }
        self.remaining -= count;
        Ok(count)
    }
}

/// Run the owned process protocol on standard streams. Actor fatal failures exit
/// the process with code 2; the controller retains pending invocations as unobserved.
/// EOF with an active transaction/cursor is incomplete, never a successful close.
///
/// # Errors
/// Invalid identity, aliases, sequencing, limits, framing or transport terminate
/// the run. Product errors instead produce their actual typed observations.
pub fn run() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let shared = Arc::new(Mutex::new(Shared {
        database: None,
        faults: fault::Hub::new(),
        observations: true,
        clock: true,
        tables: Aliases::new(),
        rows: Aliases::new(),
        observed: BTreeMap::new(),
        gates: BTreeMap::new(),
        busy: [false; wire::MAX_PENDING],
        active: 0,
        output_bytes: 0,
    }));
    let mut input = Input {
        reader: io::stdin().lock(),
        remaining: RUN_BYTES,
    };
    let mut run = None;
    let mut requests = BTreeSet::new();
    let mut actors: BTreeMap<u8, (mpsc::SyncSender<Work>, thread::JoinHandle<()>)> =
        BTreeMap::new();
    while let Some(request) = frame::read::<wire::Request>(&mut input)? {
        if run.is_none() {
            if request.request != 1
                || request.actor != 0
                || !matches!(
                    request.command.action,
                    wire::Action::ConfigureFault { .. }
                        | wire::Action::ConfigureEngine { .. }
                        | wire::Action::CreateDatabase { .. }
                        | wire::Action::OpenDatabase { .. }
                        | wire::Action::RestoreDatabase { .. }
                )
            {
                return Err(ProtocolError(
                    "first request must configure/create/open/restore as actor zero/request one",
                )
                .into());
            }
            run = Some(request.run);
        }
        if run != Some(request.run) || !requests.insert(request.request) {
            return Err(ProtocolError("run mismatch or duplicate request").into());
        }
        let is_exclusive = request.command.action.exclusive();
        let gate = {
            let mut state = lock(&shared)?;
            if state.busy[usize::from(request.actor)]
                || (is_exclusive && state.busy.iter().any(|busy| *busy))
            {
                return Err(ProtocolError(
                    "actor pending or exclusive request overlaps pending work",
                )
                .into());
            }
            let reserved = output_reservation(&request.command.action);
            if RUN_BYTES - state.output_bytes < reserved {
                return Err(ProtocolError("response budget unavailable before effect").into());
            }
            let gate = if let Some(id) = request.command.gate {
                let gate = state
                    .gates
                    .get_mut(&id)
                    .ok_or(ProtocolError("gate not configured"))?;
                let bit = 1u32 << request.actor;
                if !gate.actors.contains(&request.actor) || gate.dispatched & bit != 0 {
                    return Err(ProtocolError("gate actor absent or already used").into());
                }
                gate.dispatched |= bit;
                Some(Arc::clone(&gate.barrier))
            } else {
                None
            };
            state.output_bytes += reserved;
            state.busy[usize::from(request.actor)] = true;
            gate
        };
        if is_exclusive {
            let _scope = fault::Scope::enter(&request);
            let observation = exclusive(&shared, &request.command.action, request.run)?;
            complete(&shared, request, observation)?;
            continue;
        }
        let sender = if let Some((sender, _)) = actors.get(&request.actor) {
            sender.clone()
        } else {
            let (sender, receiver) = mpsc::sync_channel(1);
            let state = Arc::clone(&shared);
            let handle = thread::Builder::new()
                .name(format!("durable-actor-{}", request.actor))
                .spawn(move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        actor_loop(state, receiver)
                    }));
                    match result {
                        Ok(Ok(())) => (),
                        Ok(Err(error)) => {
                            eprintln!("durable actor: {error}");
                            std::process::exit(2);
                        }
                        Err(_) => {
                            eprintln!("durable actor panicked; pending effects unobserved");
                            std::process::exit(2);
                        }
                    }
                })?;
            actors.insert(request.actor, (sender.clone(), handle));
            sender
        };
        sender
            .send(Work { request, gate })
            .map_err(|_| ProtocolError("actor channel closed"))?;
    }
    if requests.is_empty() {
        return Err(ProtocolError("empty durable run").into());
    }
    let mut joins = Vec::with_capacity(actors.len());
    for (_, (sender, join)) in actors {
        drop(sender);
        joins.push(join);
    }
    for join in joins {
        join.join()
            .map_err(|_| ProtocolError("actor panicked; effects unobserved"))?;
    }
    Ok(())
}
