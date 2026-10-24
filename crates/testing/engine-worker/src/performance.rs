//! One bounded measurement fixture over the actual volatile engine API.
//! No expected value, baseline, threshold or verdict enters this module.
use crate::ProtocolError;
use std::time::Instant;
use xadb_engine::{
    api::{Column, DataType, DatabaseId, Metrics, RowUid, StoreOptions, TableHandle, TableSpec},
    diagnostics::{self, DiagnosticsMode, DiagnosticsOptions},
    substrate::{Snapshot, VolatileStore},
};
use xadb_foundation::value::Value;
use xadb_platform::performance::{process_peak_rss_bytes, thread_cpu_ns};
use xadbtest_protocol::{engine, performance as wire};

fn reserve<T>(count: usize) -> Result<Vec<T>, ProtocolError> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| ProtocolError("performance output allocation"))?;
    Ok(values)
}
fn elapsed(start: Instant) -> Result<u64, ProtocolError> {
    u64::try_from(start.elapsed().as_nanos()).map_err(|_| ProtocolError("performance clock range"))
}
fn to_mode(mode: wire::DiagnosticsMode) -> DiagnosticsMode {
    match mode {
        wire::DiagnosticsMode::Off => DiagnosticsMode::Off,
        wire::DiagnosticsMode::Always => DiagnosticsMode::Always,
        wire::DiagnosticsMode::Detailed => DiagnosticsMode::Detailed,
    }
}
fn from_mode(mode: DiagnosticsMode) -> wire::DiagnosticsMode {
    match mode {
        DiagnosticsMode::Off => wire::DiagnosticsMode::Off,
        DiagnosticsMode::Always => wire::DiagnosticsMode::Always,
        DiagnosticsMode::Detailed => wire::DiagnosticsMode::Detailed,
    }
}
fn from_operation(operation: diagnostics::Operation) -> wire::Operation {
    use diagnostics::Operation as O;
    match operation {
        O::Create => wire::Operation::Create,
        O::Drop => wire::Operation::Drop,
        O::Insert => wire::Operation::Insert,
        O::Replace => wire::Operation::Replace,
        O::Delete => wire::Operation::Delete,
        O::Snapshot => wire::Operation::Snapshot,
        O::Lookup => wire::Operation::Lookup,
        O::Schema => wire::Operation::Schema,
        O::Read => wire::Operation::Read,
        O::Scan => wire::Operation::Scan,
        O::NextPage => wire::Operation::NextPage,
    }
}
fn from_metrics(metrics: Metrics) -> engine::Metrics {
    engine::Metrics {
        operations: metrics.operations,
        rows_read: metrics.rows_read,
        row_references_copied: metrics.row_references_copied,
        resident_bytes: metrics.resident_bytes,
        snapshot_pins: metrics.snapshot_pins,
    }
}
fn from_diagnostics(snapshot: diagnostics::DiagnosticsSnapshot) -> wire::DiagnosticsSnapshot {
    wire::DiagnosticsSnapshot {
        mode: from_mode(snapshot.mode),
        event_capacity: snapshot.event_capacity as u32,
        logical_bytes: snapshot.logical_bytes as u64,
        operations: snapshot.operations.map(|operations| {
            operations.map(|counts| wire::OperationCounts {
                started: counts.started,
                completed: counts.completed,
                failed: counts.failed,
            })
        }),
        dropped_events: snapshot.dropped_events,
        saturated: snapshot.saturated,
    }
}

struct Fixture {
    store: VolatileStore,
    table: TableHandle,
    rows: Vec<RowUid>,
    state: u64,
}
impl Fixture {
    fn step(&mut self, index: u32) -> Result<i64, String> {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let row = self.rows[index as usize % self.rows.len()];
        self.store
            .replace(&self.table, row, vec![Value::Int64(self.state as i64)])
            .map_err(|error| error.to_string())?;
        let snapshot = self.store.snapshot().map_err(|error| error.to_string())?;
        // Extract only observed bytes. The controller independently computes the
        // expected scalar sequence and final/old values.
        read_value(&snapshot, &self.table, row)
        // RowView and snapshot drop before the caller takes the latency endpoint.
    }

    fn iterations(
        &mut self,
        count: u32,
        mut output: Option<&mut Samples>,
    ) -> (u32, Option<String>) {
        for index in 0..count {
            let sampled = output.is_some() && index.is_multiple_of(wire::LATENCY_STRIDE);
            let start = sampled.then(Instant::now);
            let value = match self.step(index) {
                Ok(value) => value,
                Err(error) => return (index, Some(error)),
            };
            if let Some(output) = &mut output {
                output.values.push(value);
                if let Some(start) = start {
                    match elapsed(start) {
                        Ok(ns) => output.latency.push(ns),
                        Err(error) => return (index + 1, Some(error.to_string())),
                    }
                }
            }
        }
        (count, None)
    }
}

struct Samples {
    values: Vec<i64>,
    latency: Vec<u64>,
}

fn read_value(snapshot: &Snapshot, table: &TableHandle, row: RowUid) -> Result<i64, String> {
    let view = snapshot
        .read(table, row)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "performance fixture returned an absent row".to_owned())?;
    match view.value(0) {
        Some(Value::Int64(value)) => Ok(*value),
        _ => Err("performance fixture returned a non-integer scalar".into()),
    }
}

fn values(snapshot: &Snapshot, fixture: &Fixture) -> Result<Vec<i64>, String> {
    let mut values = reserve(fixture.rows.len()).map_err(|error| error.to_string())?;
    for row in &fixture.rows {
        values.push(read_value(snapshot, &fixture.table, *row)?);
    }
    Ok(values)
}

fn retain_error(first: &mut Option<String>, next: String) {
    if first.is_none() {
        *first = Some(next);
    }
}

/// Run an isolated, single-threaded K0 batch. Invalid inputs/setup failures return
/// an execution error. A failure after setup is retained with partial observations
/// and completed counts; it cannot become a successful comparison.
pub fn run(input: &wire::Input) -> Result<wire::Output, ProtocolError> {
    if !input.validate() {
        return Err(ProtocolError("invalid performance input"));
    }
    let mut window = crate::measurement_window::Window::prepare("k0")?;
    let setup_start = Instant::now();
    let database = DatabaseId::new([2; 16]).map_err(|error| ProtocolError(error.code.id()))?;
    let mut store = VolatileStore::new_with_diagnostics(
        database,
        7,
        3,
        StoreOptions::default(),
        DiagnosticsOptions {
            mode: to_mode(input.mode),
            event_capacity: input.capacity as usize,
        },
    )
    .map_err(|error| ProtocolError(error.code.id()))?;
    let table = store
        .create_table(TableSpec {
            name: "performance".into(),
            columns: vec![Column {
                name: "value".into(),
                data_type: DataType::Int64,
                nullable: false,
            }],
        })
        .map_err(|error| ProtocolError(error.code.id()))?;
    let mut rows = reserve(input.row_count as usize)?;
    for index in 0..input.row_count {
        rows.push(
            store
                .insert(&table, vec![Value::Int64(i64::from(index))])
                .map_err(|error| ProtocolError(error.code.id()))?,
        );
    }
    let old = if input.pin_old {
        Some(
            store
                .snapshot()
                .map_err(|error| ProtocolError(error.code.id()))?,
        )
    } else {
        None
    };
    let mut fixture = Fixture {
        store,
        table,
        rows,
        state: input.seed,
    };
    let mut samples = Samples {
        values: reserve(input.measure as usize)?,
        latency: reserve((input.measure / input.latency_stride) as usize)?,
    };
    let setup_ns = elapsed(setup_start)?;
    let warmup_start = Instant::now();
    let (warmup_completed, mut error) = fixture.iterations(input.warmup, None);
    let warmup = wire::Phase {
        elapsed_ns: elapsed(warmup_start)?,
        completed: warmup_completed,
    };
    match fixture.store.drain_diagnostics() {
        Ok(drain) if drain.available => {}
        Ok(_) => retain_error(
            &mut error,
            "performance pre-measurement drain unavailable".into(),
        ),
        Err(failure) => retain_error(&mut error, failure.to_string()),
    }
    let metrics_before = from_metrics(fixture.store.metrics());
    let diagnostics_before = from_diagnostics(fixture.store.diagnostics());
    if error.is_none()
        && let Err(failure) = window.begin()
    {
        retain_error(&mut error, failure.to_string());
    }
    let (measurement, cpu_ns) = if error.is_none() {
        let cpu_before = thread_cpu_ns();
        let start = Instant::now();
        let (completed, failure) = fixture.iterations(input.measure, Some(&mut samples));
        let elapsed_ns = elapsed(start)?;
        let cpu_after = thread_cpu_ns();
        error = failure;
        if let Err(failure) = window.end(elapsed_ns) {
            // Keep the first product failure and partial observations. The
            // separate transport failure also remains in bounded worker stderr.
            eprintln!("measurement window: {failure}");
            retain_error(&mut error, failure.to_string());
        }
        (
            wire::Phase {
                elapsed_ns,
                completed,
            },
            cpu_before
                .zip(cpu_after)
                .and_then(|(before, after)| after.checked_sub(before)),
        )
    } else {
        (
            wire::Phase {
                elapsed_ns: 0,
                completed: 0,
            },
            None,
        )
    };
    let metrics_after = from_metrics(fixture.store.metrics());
    let diagnostics_after = from_diagnostics(fixture.store.diagnostics());
    let (events, events_available) = match fixture.store.drain_diagnostics() {
        Ok(drain) => {
            if !drain.available {
                retain_error(
                    &mut error,
                    "performance post-measurement drain unavailable".into(),
                );
            }
            (
                drain
                    .events
                    .into_iter()
                    .map(|event| wire::Event {
                        sequence: event.sequence,
                        operation: from_operation(event.operation),
                        error: event.error.map(|error| error.id().to_owned()),
                    })
                    .collect(),
                drain.available,
            )
        }
        Err(failure) => {
            retain_error(&mut error, failure.to_string());
            (Vec::new(), false)
        }
    };
    let final_values = fixture
        .store
        .snapshot()
        .map_err(|failure| failure.to_string())
        .and_then(|snapshot| values(&snapshot, &fixture));
    let final_values = match final_values {
        Ok(values) => values,
        Err(failure) => {
            retain_error(&mut error, failure);
            Vec::new()
        }
    };
    let old_values = old.as_ref().map(|old| match values(old, &fixture) {
        Ok(values) => values,
        Err(failure) => {
            retain_error(&mut error, failure);
            Vec::new()
        }
    });
    Ok(wire::Output {
        version: wire::VERSION,
        input: input.clone(),
        path: window.path(wire::PATH),
        build: wire::BuildIdentity {
            source_sha256: env!("XADBTEST_SOURCE_SHA256").into(),
            toolchain: env!("XADBTEST_TOOLCHAIN").into(),
            target: env!("XADBTEST_TARGET").into(),
            profile: env!("XADBTEST_PROFILE").into(),
            self_test: false,
        },
        setup_ns,
        warmup,
        measurement,
        error,
        read_values: samples.values,
        final_values,
        old_values,
        latency_ns: samples.latency,
        metrics_before,
        metrics_after,
        diagnostics_before,
        diagnostics_after,
        events,
        events_available,
        cpu_ns,
        process_peak_rss_bytes: process_peak_rss_bytes(),
    })
}
