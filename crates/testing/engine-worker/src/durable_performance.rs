//! A single retained-reader COW batch over real Linux durable ports. The worker
//! records actual values/receipts; the separate Python controller owns expectations
//! and performance verdicts. Common-mode forwarding counters remain enabled in Off.
mod ports;
use crate::ProtocolError;
use engine::{
    api::{Column, DataType, RowUid, TableHandle, TableSpec},
    database::{CommitReceipt, Engine, EngineError, EngineOptions},
    transaction::{Mutation, TransactionError, TransactionMetrics, TransactionMode},
};
use foundation::value::Value;
use platform::{
    namespace::LinuxDirectory,
    performance::{process_peak_rss_bytes, thread_cpu_ns},
    runtime::MonotonicClock,
};
use ports::{Counters, Directory, Writer};
use std::{
    fs,
    io::{self, Read, Write},
    sync::Arc,
    time::Instant,
};
use test_protocol::{
    durable, durable_performance as wire,
    frame::{self, Envelope},
};
type Database = Engine<Writer>;
#[derive(Debug)]
enum Failure {
    Product(EngineError),
    Harness(&'static str),
}
impl From<EngineError> for Failure {
    fn from(error: EngineError) -> Self {
        Self::Product(error)
    }
}
impl From<TransactionError> for Failure {
    fn from(error: TransactionError) -> Self {
        Self::Product(error.into())
    }
}
type Result<T> = std::result::Result<T, Failure>;
fn ns(start: Instant) -> u64 {
    start
        .elapsed()
        .as_nanos()
        .try_into()
        .expect("performance interval exceeds u64 nanoseconds")
}
fn reserve<T>(count: usize) -> std::result::Result<Vec<T>, ProtocolError> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| ProtocolError("durable performance output allocation"))?;
    Ok(values)
}
fn identity(db: &Database) -> wire::Identity {
    wire::Identity {
        database: db.database_id().bytes(),
        timeline: db.timeline(),
    }
}
fn receipt(receipt: CommitReceipt) -> durable::CommitIdentity {
    durable::CommitIdentity {
        transaction: crate::durable::identity(receipt.transaction),
        position: receipt.position,
        wrote: receipt.wrote,
    }
}
fn core(metrics: TransactionMetrics) -> durable::Metrics {
    durable::Metrics {
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
    }
}
fn metrics(db: &Database, counters: &Counters) -> Result<wire::Metrics> {
    Ok(wire::Metrics {
        identity: identity(db),
        core: core(db.metrics()),
        durability: crate::durable::durability_metrics(db.durability_metrics()?),
        effects: counters.snapshot(),
    })
}
fn error(stage: wire::Stage, ordinal: Option<u32>, failure: Failure) -> wire::Failure {
    match failure {
        Failure::Product(error) => {
            let durable::Observation::Error { failure } = crate::durable::engine_error(error)
            else {
                unreachable!()
            };
            wire::Failure::Product {
                stage,
                ordinal,
                error: failure,
            }
        }
        Failure::Harness(detail) => wire::Failure::Harness {
            stage,
            ordinal,
            detail: detail.into(),
        },
    }
}
fn directory(name: &str, counters: Arc<Counters>) -> Result<Directory> {
    fs::create_dir(name)
        .map_err(|_| Failure::Harness("exclusive fixture directory create failed"))?;
    let port = LinuxDirectory::open_existing(std::path::Path::new(name))
        .map_err(|_| Failure::Harness("fixture directory open failed"))?;
    Ok(Directory::new(port, counters))
}
fn scalar(
    txn: &engine::database::DurableTransaction<Writer>,
    table: &TableHandle,
    row: RowUid,
) -> Result<i64> {
    let view = txn
        .read(table, row)?
        .ok_or(Failure::Harness("observed row absent"))?;
    match view.value(0) {
        Some(Value::Int64(value)) => Ok(*value),
        _ => Err(Failure::Harness("observed scalar is not Int64")),
    }
}
fn scan(db: &Database, values: &mut Vec<i64>) -> Result<CommitReceipt> {
    let txn = db.begin(TransactionMode::ReadOnly)?;
    let table = txn.lookup_table("items")?;
    {
        let mut cursor = txn.scan(&table)?;
        loop {
            let page = cursor.next_page(wire::ROWS as usize)?;
            for row in page.rows() {
                if values.len() == wire::ROWS as usize {
                    return Err(Failure::Harness("scan exceeded fixed row bound"));
                }
                let Some(Value::Int64(value)) = row.value(0) else {
                    return Err(Failure::Harness("scan scalar is not Int64"));
                };
                values.push(*value);
            }
            if page.eof() {
                break;
            }
            if page.rows().is_empty() {
                return Err(Failure::Harness("scan made no progress"));
            }
        }
    }
    Ok(txn.commit()?)
}
// Keep the fixed fixture inputs and the preallocated observation buffers
// explicit; warmup and measurement use the same production cycle.
#[allow(clippy::too_many_arguments)]
fn iterations(
    db: &Database,
    table: &TableHandle,
    rows: &[RowUid],
    state: &mut u64,
    start_ordinal: u32,
    count: u32,
    batch: &mut wire::Batch,
    stage: wire::Stage,
) -> Option<wire::Failure> {
    let phase_start = Instant::now();
    for offset in 0..count {
        let ordinal = start_ordinal + offset;
        *state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let value = *state as i64;
        let mut item = wire::Iteration {
            ordinal,
            begin: None,
            commit: None,
            read_value: None,
            read_commit: None,
            elapsed_ns: 0,
            commit_ns: None,
            readback_ns: None,
            completed: false,
        };
        let started = Instant::now();
        let result = (|| -> Result<()> {
            let mut txn = db.begin(TransactionMode::ReadWrite)?;
            item.begin = Some(crate::durable::identity(txn.id()));
            txn.execute_statement(vec![Mutation::Replace {
                table: table.clone(),
                row: rows[ordinal as usize % wire::ROWS as usize],
                values: vec![Value::Int64(value)],
            }])?;
            let commit_start = Instant::now();
            let committed = txn.commit();
            item.commit_ns = Some(ns(commit_start));
            item.commit = Some(receipt(committed?));
            let read_start = Instant::now();
            let result = (|| -> Result<()> {
                let read = db.begin(TransactionMode::ReadOnly)?;
                item.read_value = Some(scalar(
                    &read,
                    table,
                    rows[ordinal as usize % wire::ROWS as usize],
                )?);
                item.read_commit = Some(receipt(read.commit()?));
                Ok(())
            })();
            item.readback_ns = Some(ns(read_start));
            result
        })();
        item.elapsed_ns = ns(started);
        item.completed = result.is_ok();
        batch.iterations.push(item);
        if let Err(failure) = result {
            batch.elapsed_ns = ns(phase_start);
            return Some(error(stage, Some(ordinal), failure));
        }
        batch.completed += 1;
    }
    batch.elapsed_ns = ns(phase_start);
    None
}
fn affinity() -> Option<String> {
    let mut bytes = String::new();
    fs::File::open("/proc/self/status")
        .ok()?
        .take(65537)
        .read_to_string(&mut bytes)
        .ok()?;
    if bytes.len() > 65536 {
        return None;
    }
    bytes.lines().find_map(|line| {
        line.strip_prefix("Cpus_allowed_list:")
            .map(|value| value.trim().to_owned())
    })
}
fn output(input: wire::Input) -> std::result::Result<wire::Output, ProtocolError> {
    Ok(wire::Output {
        version: wire::VERSION,
        path: wire::PATH.into(),
        input,
        build: test_protocol::performance::BuildIdentity {
            source_sha256: env!("XADBTEST_SOURCE_SHA256").into(),
            toolchain: env!("XADBTEST_TOOLCHAIN").into(),
            target: env!("XADBTEST_TARGET").into(),
            profile: env!("XADBTEST_PROFILE").into(),
            self_test: false,
        },
        directories: wire::Directories {
            source: "source".into(),
            backup: "backup".into(),
            restored: "restored".into(),
        },
        setup: wire::Setup {
            elapsed_ns: 0,
            identity: None,
            table: None,
            ddl: None,
            insert_begin: None,
            insert_commit: None,
            rows: reserve(wire::ROWS as usize)?,
        },
        warmup: wire::Batch {
            elapsed_ns: 0,
            completed: 0,
            iterations: reserve(wire::WARMUP as usize)?,
        },
        measurement: wire::Batch {
            elapsed_ns: 0,
            completed: 0,
            iterations: reserve(wire::MEASURE as usize)?,
        },
        metrics_before: None,
        metrics_after: None,
        old_values: reserve(wire::ROWS as usize)?,
        old_commit: None,
        maintenance: reserve(wire::PHASE_COUNT)?,
        error: None,
        actual_affinity: affinity(),
        cpu_ns: None,
        process_peak_rss_bytes: None,
    })
}
fn execute(
    out: &mut wire::Output,
    phase_values: &mut [Vec<i64>],
    stage_now: &mut wire::Stage,
    window: &mut crate::measurement_window::Window,
) -> Result<()> {
    let counters = Counters::new();
    let options = EngineOptions {
        observation_enabled: out.input.observations,
        clock: Some(Arc::new(MonotonicClock::default())),
        ..EngineOptions::default()
    };
    let setup_start = Instant::now();
    let setup = (|| -> Result<_> {
        let source = directory("source", Arc::clone(&counters))?;
        let db = Database::create(&source, options.clone())?;
        out.setup.identity = Some(identity(&db));
        let (table, ddl) = db.create_table_with_receipt(TableSpec {
            name: "items".into(),
            columns: vec![Column {
                name: "value".into(),
                data_type: DataType::Int64,
                nullable: false,
            }],
        })?;
        out.setup.ddl = Some(receipt(ddl));
        out.setup.table = Some(crate::table_identity(
            &table,
            db.database_id().bytes(),
            db.timeline(),
        ));
        let mut insert = db.begin(TransactionMode::ReadWrite)?;
        out.setup.insert_begin = Some(crate::durable::identity(insert.id()));
        let inserted = insert.execute_statement(
            (0..wire::ROWS)
                .map(|index| Mutation::Insert {
                    table: table.clone(),
                    values: vec![Value::Int64(index as i64)],
                })
                .collect(),
        )?;
        for row in &inserted.inserted {
            if out.setup.rows.len() == wire::ROWS as usize {
                return Err(Failure::Harness("setup row result exceeded fixed bound"));
            }
            out.setup.rows.push(crate::row_identity(*row));
        }
        out.setup.insert_commit = Some(receipt(insert.commit()?));
        if inserted.inserted.len() != wire::ROWS as usize {
            return Err(Failure::Harness(
                "setup inserted row count differs from bound",
            ));
        }
        Ok((source, db, table, inserted.inserted))
    })();
    out.setup.elapsed_ns = ns(setup_start);
    let (source, db, table, rows) = setup?;
    let old = db.begin(TransactionMode::ReadOnly)?;
    let mut state = out.input.seed;
    *stage_now = wire::Stage::Warmup;
    out.error = iterations(
        &db,
        &table,
        &rows,
        &mut state,
        0,
        wire::WARMUP,
        &mut out.warmup,
        wire::Stage::Warmup,
    );
    out.metrics_before = Some(metrics(&db, &counters)?);
    if out.error.is_none() {
        *stage_now = wire::Stage::Measurement;
        window.begin().map_err(|error| Failure::Harness(error.0))?;
        let cpu_before = thread_cpu_ns();
        out.error = iterations(
            &db,
            &table,
            &rows,
            &mut state,
            wire::WARMUP,
            wire::MEASURE,
            &mut out.measurement,
            wire::Stage::Measurement,
        );
        out.cpu_ns = cpu_before
            .zip(thread_cpu_ns())
            .and_then(|(before, after)| after.checked_sub(before));
        window.end(out.measurement.elapsed_ns).map_err(|error| {
            eprintln!("measurement window: {error}");
            Failure::Harness(error.0)
        })?;
    }
    out.metrics_after = Some(metrics(&db, &counters)?);
    if out.error.is_some() {
        return Ok(());
    }
    *stage_now = wire::Stage::Scan;
    for row in &rows {
        out.old_values.push(scalar(&old, &table, *row)?);
    }
    out.old_commit = Some(receipt(old.commit()?));
    let mut source_db = Some(db);
    for (index, stage) in [
        wire::Stage::Scan,
        wire::Stage::Checkpoint,
        wire::Stage::GarbageCollect,
        wire::Stage::Backup,
        wire::Stage::Restore,
        wire::Stage::Reopen,
    ]
    .into_iter()
    .enumerate()
    {
        *stage_now = stage;
        let mut phase = wire::Maintenance {
            stage,
            elapsed_ns: 0,
            completed: false,
            identity: None,
            position: None,
            generation: None,
            files: None,
            bytes: None,
            values: std::mem::take(&mut phase_values[index]),
            metrics_before: None,
            metrics_after: None,
            effects_before: counters.snapshot(),
            effects_after: counters.snapshot(),
        };
        let result = (|| -> Result<()> {
            phase.metrics_before = Some(metrics(
                source_db.as_ref().expect("source engine before reopen"),
                &counters,
            )?);
            let started = Instant::now();
            let result = (|| -> Result<()> {
                match stage {
                    wire::Stage::Scan => {
                        let db = source_db.as_ref().expect("source engine before phase");
                        phase.identity = Some(identity(db));
                        phase.position = Some(scan(db, &mut phase.values)?.position);
                    }
                    wire::Stage::Checkpoint => {
                        let db = source_db.as_ref().expect("source engine before phase");
                        let receipt = db.checkpoint()?;
                        phase.generation = Some(receipt.generation);
                        phase.position = Some(receipt.position);
                        phase.files = Some(receipt.files as u64);
                        phase.bytes = Some(receipt.bytes);
                    }
                    wire::Stage::GarbageCollect => {
                        let db = source_db.as_ref().expect("source engine before phase");
                        let receipt = db.collect_garbage()?;
                        phase.files = Some(receipt.files as u64);
                        phase.bytes = Some(receipt.bytes);
                    }
                    wire::Stage::Backup => {
                        let db = source_db.as_ref().expect("source engine before phase");
                        let backup = directory("backup", Arc::clone(&counters))?;
                        let receipt = db.backup(&backup)?;
                        phase.generation = Some(receipt.generation);
                        phase.position = Some(receipt.position);
                        phase.files = Some(receipt.files as u64);
                        phase.bytes = Some(receipt.bytes);
                    }
                    wire::Stage::Restore => {
                        let backup = Directory::new(
                            LinuxDirectory::open_existing(std::path::Path::new("backup"))
                                .map_err(|_| Failure::Harness("backup directory reopen failed"))?,
                            Arc::clone(&counters),
                        );
                        let target = directory("restored", Arc::clone(&counters))?;
                        let restored = Database::restore_new(&backup, &target, options.clone())?;
                        phase.identity = Some(identity(&restored));
                        phase.position = Some(scan(&restored, &mut phase.values)?.position);
                        phase.metrics_after = Some(metrics(&restored, &counters)?);
                    }
                    wire::Stage::Reopen => {
                        drop(source_db.take());
                        let reopened = Database::open(&source, options.clone())?;
                        phase.identity = Some(identity(&reopened));
                        phase.position = Some(scan(&reopened, &mut phase.values)?.position);
                        phase.metrics_after = Some(metrics(&reopened, &counters)?);
                        source_db = Some(reopened);
                    }
                    _ => unreachable!(),
                }
                Ok(())
            })();
            phase.elapsed_ns = ns(started);
            result?;
            if phase.metrics_after.is_none() {
                phase.metrics_after = Some(metrics(
                    source_db.as_ref().expect("source engine after phase"),
                    &counters,
                )?);
            }
            Ok(())
        })();
        phase.effects_after = counters.snapshot();
        phase.completed = result.is_ok();
        out.maintenance.push(phase);
        if let Err(failure) = result {
            out.error = Some(error(stage, None, failure));
            break;
        }
    }
    Ok(())
}
/// Process exactly one bounded batch; EOF after the response is required. Fatal
/// output/framing failure leaves the invocation unobserved rather than aborted.
/// # Errors
/// Invalid input, pre-effect allocation, framing and transport failures are fatal.
pub fn run() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let request: wire::Request =
        frame::read(&mut io::stdin().lock())?.ok_or("missing performance request")?;
    let mut encoded: Vec<u8> = reserve(wire::OUTPUT_CAP)?;
    encoded.resize(4, 0);
    let mut out = output(request.input)?;
    let mut values: Vec<Vec<i64>> = reserve(wire::PHASE_COUNT)?;
    for _ in 0..wire::PHASE_COUNT {
        values.push(reserve(wire::ROWS as usize)?);
    }
    let mut window = crate::measurement_window::Window::prepare("durable")?;
    out.path = window.path(wire::PATH);
    let mut stage_now = wire::Stage::Setup;
    if let Err(failure) = execute(&mut out, &mut values, &mut stage_now, &mut window)
        && out.error.is_none()
    {
        out.error = Some(error(stage_now, None, failure));
    }
    out.process_peak_rss_bytes = process_peak_rss_bytes();
    let mut response = wire::Response {
        version: wire::VERSION,
        run: request.run,
        request: request.request,
        actor: request.actor,
        output: out,
    };
    if response.output.path == wire::PATH {
        response.validate()?;
    } else {
        if response.output.path != window.path(wire::PATH) {
            return Err(ProtocolError("unexpected diagnostic output path").into());
        }
        // Reuse all ordinary output bounds on a temporary path, then restore
        // the diagnostic identity before serialization. Ordinary readers must
        // still reject these raw diagnostic bytes; the protocol is unchanged.
        let diagnostic_path = std::mem::replace(&mut response.output.path, wire::PATH.into());
        let validation = response.validate();
        response.output.path = diagnostic_path;
        validation?;
    }
    struct Bounded<'a>(&'a mut Vec<u8>);
    impl Write for Bounded<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > wire::OUTPUT_CAP - self.0.len() {
                return Err(io::Error::other("performance response bound"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Bounded(&mut encoded), &response)?;
    let length = (encoded.len() - 4) as u32;
    encoded[..4].copy_from_slice(&length.to_be_bytes());
    io::stdout().write_all(&encoded)?;
    io::stdout().flush()?;
    let mut byte = [0u8; 1];
    if io::stdin().read(&mut byte)? != 0 {
        return Err("performance worker accepts exactly one batch".into());
    }
    Ok(())
}
