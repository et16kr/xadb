//! Real Linux file I/O with two conflicting writers and four old/fresh readers.
//! Literal driver values and fixed phase barriers check atomicity and ACK replay;
//! this does not simulate power loss or replace the independent WAL oracle.
use engine::{
    api::{Column, DataType, RowUid, TableHandle, TableSpec},
    database::{CommitReceipt, Engine, EngineError, EngineOptions},
    transaction::{Mutation, TransactionEffect, TransactionErrorCode, TransactionMode},
};
use foundation::value::Value;
use platform::{
    namespace::{LinuxDirectory, LinuxWriterLease},
    runtime::MonotonicClock,
};
use std::{
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicI64, AtomicU8, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

const GENERATOR: &str = "dev2-durable-thread-v1";
const READERS: usize = 4;
const FIXED_READERS: usize = READERS - 1;
const MAX_PHASES: usize = 512;
struct DoneSignal<'a>(&'a AtomicBool);
impl Drop for DoneSignal<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
struct CommitActive<'a>(&'a AtomicUsize);
impl<'a> CommitActive<'a> {
    fn enter(active: &'a AtomicUsize) -> Self {
        active.fetch_add(1, Ordering::SeqCst);
        Self(active)
    }
}
impl Drop for CommitActive<'_> {
    fn drop(&mut self) {
        assert!(self.0.fetch_sub(1, Ordering::SeqCst) > 0);
    }
}
struct Samples {
    checks: u64,
    overlap_begin: u64,
    overlap_end: u64,
    overlap_both: u64,
    seen_actor_masks: [u8; MAX_PHASES],
}
impl Default for Samples {
    fn default() -> Self {
        Self {
            checks: 0,
            overlap_begin: 0,
            overlap_end: 0,
            overlap_both: 0,
            seen_actor_masks: [0; MAX_PHASES],
        }
    }
}
struct Gate {
    count: usize,
    state: Mutex<(usize, u64)>,
    changed: Condvar,
}
impl Gate {
    fn new(count: usize) -> Self {
        Self {
            count,
            state: Mutex::new((0, 0)),
            changed: Condvar::new(),
        }
    }
    fn wait(&self) {
        let mut state = self.state.lock().unwrap();
        let generation = state.1;
        state.0 += 1;
        if state.0 == self.count {
            state.0 = 0;
            state.1 += 1;
            self.changed.notify_all();
            return;
        }
        let (state, timeout) = self
            .changed
            .wait_timeout_while(state, Duration::from_secs(15), |state| {
                state.1 == generation
            })
            .unwrap();
        drop(state);
        assert!(!timeout.timed_out(), "real-thread rendezvous deadline");
    }
}
struct Fixture(Option<tempfile::TempDir>);
impl Fixture {
    fn new() -> Self {
        let directory = if let Some(path) = std::env::var_os("XADBTEST_V3_FAILURE_DIR") {
            tempfile::Builder::new()
                .prefix("kernel-")
                .tempdir_in(path)
                .unwrap()
        } else {
            tempfile::tempdir().unwrap()
        };
        Self(Some(directory))
    }
    fn path(&self) -> &std::path::Path {
        self.0.as_ref().unwrap().path()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if std::thread::panicking()
            && let Some(directory) = self.0.take()
        {
            eprintln!("retained kernel failure: {}", directory.keep().display());
        }
    }
}
fn schema(name: &str) -> TableSpec {
    TableSpec {
        name: name.into(),
        columns: vec![Column {
            name: "value".into(),
            data_type: DataType::Int64,
            nullable: false,
        }],
    }
}
fn observed(
    txn: &engine::database::DurableTransaction<LinuxWriterLease>,
    tables: &[TableHandle; 2],
    rows: [RowUid; 2],
    expected: i64,
) {
    for index in 0..2 {
        assert_eq!(
            txn.read(&tables[index], rows[index])
                .unwrap()
                .unwrap()
                .value(0),
            Some(&Value::Int64(expected))
        );
    }
}
fn proposal(seed: usize, phase: usize, actor: usize) -> i64 {
    1 + (seed as i64 * 2048) + phase as i64 * 2 + actor as i64
}

fn one_round(seed: usize, phases: usize, enabled: bool) -> (u64, u64, Samples) {
    assert!(phases <= MAX_PHASES);
    let directory = Fixture::new();
    let port = LinuxDirectory::open_existing(directory.path()).unwrap();
    let options = EngineOptions {
        observation_enabled: enabled,
        clock: Some(Arc::new(MonotonicClock::default())),
        ..EngineOptions::default()
    };
    let db = Engine::create(&port, options.clone()).unwrap();
    let tables = [
        db.create_table(schema("left_rows")).unwrap(),
        db.create_table(schema("right_rows")).unwrap(),
    ];
    let initial = -(seed as i64) - 1;
    let mut txn = db.begin(TransactionMode::ReadWrite).unwrap();
    let rows: [RowUid; 2] = txn
        .execute_statement(
            tables
                .iter()
                .map(|table| Mutation::Insert {
                    table: table.clone(),
                    values: vec![Value::Int64(initial)],
                })
                .collect(),
        )
        .unwrap()
        .inserted
        .try_into()
        .unwrap();
    assert_eq!(txn.commit().unwrap().position, 3);
    let expected = AtomicI64::new(initial);
    let pair = Gate::new(2);
    let start = Gate::new(READERS + 2);
    let phase_gate = Gate::new(FIXED_READERS + 2);
    let decisions = Mutex::new([None::<CommitReceipt>; 2]);
    let done = AtomicBool::new(false);
    let in_commit = AtomicUsize::new(0);
    let winner_masks: [AtomicU8; MAX_PHASES] = std::array::from_fn(|_| AtomicU8::new(0));
    let samples = std::thread::scope(|scope| {
        for actor in 0..2 {
            let db = &db;
            let tables = &tables;
            let expected = &expected;
            let pair = &pair;
            let start = &start;
            let phase_gate = &phase_gate;
            let decisions = &decisions;
            let done = &done;
            let in_commit = &in_commit;
            let winner_masks = &winner_masks;
            scope.spawn(move || {
                // Lazy construction matters: the other writer must not drop a
                // temporary DoneSignal and prematurely stop the sampler.
                let _done = (actor == 0).then(|| DoneSignal(done));
                start.wait();
                for (phase, winner_mask) in winner_masks.iter().enumerate().take(phases) {
                    let mut txn = db.begin(TransactionMode::ReadWrite).unwrap();
                    observed(&txn, tables, rows, expected.load(Ordering::Acquire));
                    txn.execute_statement(
                        (0..2)
                            .map(|index| Mutation::Replace {
                                table: tables[index].clone(),
                                row: rows[index],
                                values: vec![Value::Int64(proposal(seed, phase, actor))],
                            })
                            .collect(),
                    )
                    .unwrap();
                    pair.wait(); // Both read the same committed q before either commit.
                    let result = {
                        let _active = CommitActive::enter(in_commit);
                        txn.commit()
                    };
                    let result = match result {
                        Ok(receipt) => {
                            assert!(receipt.wrote);
                            Some(receipt)
                        }
                        Err(EngineError::Transaction(error)) => {
                            assert_eq!(error.code, TransactionErrorCode::SerializationConflict);
                            assert_eq!(error.effect, TransactionEffect::Aborted);
                            None
                        }
                        Err(error) => panic!("unexpected durable commit failure: {error:?}"),
                    };
                    decisions.lock().unwrap()[actor] = result;
                    pair.wait();
                    if actor == 0 {
                        let results = *decisions.lock().unwrap();
                        assert_eq!(results.iter().flatten().count(), 1);
                        let winner = usize::from(results[0].is_none());
                        let receipt = results[winner].unwrap();
                        assert_eq!(receipt.position, 4 + phase as u64);
                        winner_mask.store(1 << winner, Ordering::Release);
                        expected.store(proposal(seed, phase, winner), Ordering::Release);
                        if phase % 64 == 63 {
                            assert_eq!(db.checkpoint().unwrap().position, receipt.position);
                            db.collect_garbage().unwrap();
                        }
                    }
                    pair.wait();
                    phase_gate.wait();
                    phase_gate.wait();
                }
            });
        }
        for _ in 0..FIXED_READERS {
            let db = &db;
            let tables = &tables;
            let expected = &expected;
            let start = &start;
            let phase_gate = &phase_gate;
            scope.spawn(move || {
                let old = db.begin(TransactionMode::ReadOnly).unwrap();
                start.wait();
                for phase in 0..phases {
                    phase_gate.wait(); // The new two-table commit is already published.
                    observed(&old, tables, rows, initial);
                    let current = db.begin(TransactionMode::ReadOnly).unwrap();
                    observed(&current, tables, rows, expected.load(Ordering::Acquire));
                    let receipt = current.commit().unwrap();
                    assert!(!receipt.wrote);
                    assert_eq!(receipt.position, 4 + phase as u64);
                    phase_gate.wait();
                }
                let receipt = old.commit().unwrap();
                assert!(!receipt.wrote);
                assert_eq!(receipt.position, 3);
            });
        }
        let db = &db;
        let tables = &tables;
        let start = &start;
        let done = &done;
        let in_commit = &in_commit;
        scope
            .spawn(move || {
                let mut samples = Samples::default();
                start.wait();
                loop {
                    // These counters witness overlap with writer commit invocation
                    // intervals. They do not identify an internal WAL/publication cut.
                    let before = in_commit.load(Ordering::SeqCst) != 0;
                    let txn = db.begin(TransactionMode::ReadOnly).unwrap();
                    let mut values = [0; 2];
                    for index in 0..2 {
                        let row = txn.read(&tables[index], rows[index]).unwrap().unwrap();
                        let Some(Value::Int64(value)) = row.value(0) else {
                            panic!("sampler expected a single integer scalar");
                        };
                        values[index] = *value;
                    }
                    let receipt = txn.commit().unwrap();
                    let after = in_commit.load(Ordering::SeqCst) != 0;
                    assert!(!receipt.wrote);
                    assert_eq!(values[0], values[1], "two-table snapshot is atomic");
                    if receipt.position == 3 {
                        assert_eq!(values, [initial; 2]);
                    } else {
                        assert!((4..=3 + phases as u64).contains(&receipt.position));
                        let phase = (receipt.position - 4) as usize;
                        let actor = if values[0] == proposal(seed, phase, 0) {
                            0
                        } else {
                            assert_eq!(values[0], proposal(seed, phase, 1));
                            1
                        };
                        samples.seen_actor_masks[phase] |= 1 << actor;
                    }
                    samples.checks += 1;
                    samples.overlap_begin += u64::from(before);
                    samples.overlap_end += u64::from(after);
                    samples.overlap_both += u64::from(before && after);
                    if done.load(Ordering::Acquire) {
                        break;
                    }
                    std::thread::yield_now();
                }
                samples
            })
            .join()
            .unwrap()
    });
    assert!(samples.checks > 0);
    assert_eq!(in_commit.load(Ordering::SeqCst), 0);
    for (phase, winner) in winner_masks[..phases].iter().enumerate() {
        let winner = winner.load(Ordering::Acquire);
        assert!(winner == 1 || winner == 2);
        assert_eq!(
            samples.seen_actor_masks[phase] & !winner,
            0,
            "only the acknowledged winner may be sampled"
        );
    }
    let metrics = db.metrics();
    assert_eq!(metrics.private_bytes, 0);
    assert_eq!(metrics.active_transactions, 0);
    assert_eq!(metrics.substrate.snapshot_pins, 0);
    let expected = expected.load(Ordering::Acquire);
    drop(db);
    let db = Engine::open(&port, options).unwrap();
    let txn = db.begin(TransactionMode::ReadOnly).unwrap();
    let tables = [
        txn.lookup_table("left_rows").unwrap(),
        txn.lookup_table("right_rows").unwrap(),
    ];
    observed(&txn, &tables, rows, expected);
    assert_eq!(txn.commit().unwrap().position, 3 + phases as u64);
    (
        phases as u64,
        (FIXED_READERS * phases * 4) as u64 + 2 * samples.checks,
        samples,
    )
}

fn run(extended: bool) {
    let seed_count = if extended { 256 } else { 64 };
    let phases = if extended { 512 } else { 64 };
    let minimum = if extended { 1800 } else { 0 };
    println!(
        "XADB_KERNEL_THREAD_PROFILE {{\"generator\":\"{GENERATOR}\",\"seed_count\":{seed_count},\"writer_attempts_per_seed\":{},\"readers\":4,\"writers\":2,\"min_seconds\":{minimum},\"power_loss\":false}}",
        phases * 2
    );
    let began = Instant::now();
    let mut progress = Instant::now();
    let mut rounds = 0_u64;
    let mut commits = 0;
    let mut reads = 0;
    let mut sampler_checks = 0;
    let mut overlap_begin = 0;
    let mut overlap_end = 0;
    let mut overlap_both = 0;
    let mut visits = [0_u64; 256];
    loop {
        let seed = rounds as usize % seed_count;
        let (written, read, samples) = one_round(seed, phases, seed.is_multiple_of(2));
        visits[seed] += 1;
        commits += written;
        reads += read;
        sampler_checks += samples.checks;
        overlap_begin += samples.overlap_begin;
        overlap_end += samples.overlap_end;
        overlap_both += samples.overlap_both;
        rounds += 1;
        if rounds >= seed_count as u64 && began.elapsed() >= Duration::from_secs(minimum) {
            break;
        }
        if progress.elapsed() >= Duration::from_secs(30) {
            println!(
                "XADB_KERNEL_THREAD_PROGRESS rounds={rounds} elapsed_ms={}",
                began.elapsed().as_millis()
            );
            progress = Instant::now();
        }
    }
    assert!(visits[..seed_count].iter().all(|count| *count > 0));
    assert!(
        overlap_both > 0,
        "INCOMPLETE: no sampler interval overlapped commit invocations at both observations"
    );
    println!(
        "XADB_KERNEL_THREAD_RESULT {{\"generator\":\"{GENERATOR}\",\"elapsed_ms\":{},\"rounds\":{rounds},\"commits\":{commits},\"conflicts\":{commits},\"reader_row_checks\":{reads},\"sampler_checks\":{sampler_checks},\"sampler_overlap_begin\":{overlap_begin},\"sampler_overlap_end\":{overlap_end},\"sampler_overlap_both\":{overlap_both},\"seed_visits\":{:?}}}",
        began.elapsed().as_millis(),
        &visits[..seed_count]
    );
}
#[test]
fn durable_two_writers_four_readers_v2_64_seeds() {
    run(false);
}
#[test]
#[ignore = "Requires retained release binaries and at least 1800 seconds"]
fn durable_two_writers_four_readers_v3_30_minutes() {
    run(true);
}
