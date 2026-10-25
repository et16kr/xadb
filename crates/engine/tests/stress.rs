//! Bounded K0 real-thread stress over the public volatile API.
//!
//! One writer owns `&mut VolatileStore`; four readers own four separately acquired
//! immutable snapshots. This exercises lifetime/ledger and diagnostic contention,
//! not transaction isolation or durability. The V3 test fixes its 1,800 second
//! lower bound in code; the evidence wrapper owns timeout and raw failure logs.

use engine::{
    ErrorCode, Result,
    api::{Column, DataType, DatabaseId, RowUid, StoreOptions, TableHandle, TableSpec},
    diagnostics::{
        DiagnosticEvent, DiagnosticsMode, DiagnosticsOptions, OPERATION_COUNT, Operation,
        OperationCounts,
    },
    substrate::{RowView, ScanPage, Snapshot, VolatileStore},
};
use foundation::value::Value;
use std::{
    collections::BTreeSet,
    sync::{Arc, Barrier},
    thread,
    time::{Duration, Instant},
};

const READERS: usize = 4;
const ROWS: usize = 16;
const PHASES: usize = 3;
const READER_PASSES: usize = 8;
const MAX_ROUNDS: u64 = 10_000_000;
const GENERATOR: &str = "k0-thread-v1";
const TARGET: &str = "k0_real_thread_stress";
const LANES: [DiagnosticsOptions; 5] = [
    DiagnosticsOptions {
        mode: DiagnosticsMode::Off,
        event_capacity: 256,
    },
    DiagnosticsOptions {
        mode: DiagnosticsMode::Always,
        event_capacity: 256,
    },
    DiagnosticsOptions {
        mode: DiagnosticsMode::Detailed,
        event_capacity: 0,
    },
    DiagnosticsOptions {
        mode: DiagnosticsMode::Detailed,
        event_capacity: 1,
    },
    DiagnosticsOptions {
        mode: DiagnosticsMode::Detailed,
        event_capacity: 256,
    },
];

/// Expected invocation counts come from the driver, never from engine counters.
#[derive(Default)]
struct Invocations([OperationCounts; OPERATION_COUNT]);

impl Invocations {
    fn success<T>(&mut self, operation: Operation, result: Result<T>) -> T {
        let counts = &mut self.0[operation as usize];
        counts.started += 1;
        counts.completed += 1;
        result.unwrap_or_else(|error| panic!("unexpected {operation:?} failure: {error:?}"))
    }

    fn failure<T>(&mut self, operation: Operation, result: Result<T>, expected: ErrorCode) {
        let counts = &mut self.0[operation as usize];
        counts.started += 1;
        counts.completed += 1;
        counts.failed += 1;
        assert_eq!(
            result
                .err()
                .expect("required product error was absent")
                .code,
            expected,
            "{operation:?}"
        );
    }

    fn merge(&mut self, other: &Self) {
        for (ours, theirs) in self.0.iter_mut().zip(other.0.iter()) {
            ours.started += theirs.started;
            ours.completed += theirs.completed;
            ours.failed += theirs.failed;
        }
    }

    fn total(&self) -> u64 {
        self.0.iter().map(|counts| counts.completed).sum()
    }
}

fn schema() -> TableSpec {
    TableSpec {
        name: "rows".into(),
        columns: vec![Column {
            name: "value".into(),
            data_type: DataType::Int64,
            nullable: false,
        }],
    }
}

/// A scalar fixture formula independent of production roots, codecs and compare.
fn original(seed: u64, row: usize) -> i64 {
    i64::try_from(seed * 1_000 + u64::try_from(row).unwrap()).unwrap() - 128_000
}

fn check_view(view: &RowView, id: RowUid, expected: i64) {
    assert_eq!(view.uid(), id);
    assert_eq!(view.len(), 1);
    assert!(!view.is_empty());
    assert_eq!(view.value(0), Some(&Value::Int64(expected)));
    assert_eq!(view.value(1), None);
}

fn check_page(page: &ScanPage, ids: &[RowUid], seed: u64, start: usize, eof: bool) {
    assert_eq!(page.rows().len(), ids.len());
    // Independent literal layout: count(4) + Int64 tag(1) + value(8) + UID(56).
    assert_eq!(page.canonical_bytes(), ids.len() * 69);
    assert_eq!(page.eof(), eof);
    for (offset, (view, id)) in page.rows().iter().zip(ids.iter()).enumerate() {
        check_view(view, *id, original(seed, start + offset));
    }
}

fn check_old_snapshot(
    snapshot: &Snapshot,
    table: &TableHandle,
    ids: &[RowUid; ROWS],
    seed: u64,
    counts: &mut Invocations,
) {
    assert_eq!(
        counts.success(Operation::Lookup, snapshot.lookup_table("rows")),
        *table
    );
    counts.failure(
        Operation::Lookup,
        snapshot.lookup_table("absent"),
        ErrorCode::TableNotFound,
    );
    assert_eq!(
        counts.success(Operation::Schema, snapshot.schema(table)),
        &schema()
    );
    for (row, id) in ids.iter().enumerate() {
        let view = counts
            .success(Operation::Read, snapshot.read(table, *id))
            .expect("fixed snapshot row disappeared");
        check_view(&view, *id, original(seed, row));
    }
    let mut cursor = counts.success(Operation::Scan, snapshot.scan(table));
    counts.failure(
        Operation::NextPage,
        cursor.next_page(0),
        ErrorCode::LimitExceeded,
    );
    for start in (0..ROWS).step_by(3) {
        let end = (start + 3).min(ROWS);
        let page = counts.success(Operation::NextPage, cursor.next_page(3));
        check_page(&page, &ids[start..end], seed, start, end == ROWS);
    }
    let page = counts.success(Operation::NextPage, cursor.next_page(3));
    check_page(&page, &[], seed, ROWS, true);
}

fn reader(
    index: usize,
    seed: u64,
    snapshot: Snapshot,
    table: TableHandle,
    ids: [RowUid; ROWS],
    barrier: Arc<Barrier>,
) -> Invocations {
    let mut counts = Invocations::default();
    let view = counts
        .success(Operation::Read, snapshot.read(&table, ids[ROWS - 1]))
        .unwrap();
    let retained_view = view.clone();
    drop(view);
    let mut cursor = counts.success(Operation::Scan, snapshot.scan(&table));
    let page = counts.success(Operation::NextPage, cursor.next_page(2));
    check_page(&page, &ids[..2], seed, 0, false);

    for phase in 0..PHASES {
        barrier.wait();
        for pass in 0..READER_PASSES {
            check_old_snapshot(&snapshot, &table, &ids, seed, &mut counts);
            if (seed + (index + phase + pass) as u64).is_multiple_of(2) {
                thread::yield_now();
            }
        }
        barrier.wait();
    }

    // The table was dropped/recreated before this check. Cursor and result owners
    // must keep the old allocation alive after the source snapshot is destroyed.
    drop(snapshot);
    let tail = counts.success(Operation::NextPage, cursor.next_page(3));
    check_page(&tail, &ids[2..5], seed, 2, false);
    drop(tail);
    drop(cursor);
    check_page(&page, &ids[..2], seed, 0, false);
    check_view(&retained_view, ids[ROWS - 1], original(seed, ROWS - 1));
    barrier.wait();
    barrier.wait();

    drop(page);
    check_view(&retained_view, ids[ROWS - 1], original(seed, ROWS - 1));
    barrier.wait();
    barrier.wait();

    drop(retained_view);
    barrier.wait();
    barrier.wait();
    counts
}

/// Only bounded per-round buffers are retained. No live snapshot is interpreted
/// as a coherent counter cut while completion counters can still advance.
fn drain(store: &VolatileStore, events: &mut Vec<DiagnosticEvent>) {
    let snapshot = store.diagnostics();
    assert_ne!(snapshot.saturated, Some(true));
    let drained = store.drain_diagnostics().unwrap();
    assert!(drained.events.len() <= snapshot.event_capacity);
    if !drained.available {
        assert!(drained.events.is_empty());
    }
    events.extend(drained.events);
}

fn verify_diagnostics(
    store: &VolatileStore,
    options: DiagnosticsOptions,
    expected: &Invocations,
    events: &[DiagnosticEvent],
) {
    let snapshot = store.diagnostics();
    assert_eq!(snapshot.mode, options.mode);
    assert_eq!(snapshot.event_capacity, options.event_capacity);
    match options.mode {
        DiagnosticsMode::Off => {
            assert_eq!(snapshot.logical_bytes, 0);
            assert_eq!(snapshot.operations, None);
            assert_eq!(snapshot.saturated, None);
            assert_eq!(snapshot.dropped_events, None);
            assert!(events.is_empty());
        }
        DiagnosticsMode::Always | DiagnosticsMode::Detailed => {
            assert!(snapshot.logical_bytes > 0);
            assert_eq!(snapshot.operations, Some(expected.0));
            assert_eq!(snapshot.saturated, Some(false));
            if options.mode == DiagnosticsMode::Always {
                assert_eq!(snapshot.dropped_events, None);
                assert!(events.is_empty());
                return;
            }
            assert_eq!(
                events.len() as u64 + snapshot.dropped_events.unwrap(),
                expected.total()
            );
            let mut sequences = BTreeSet::new();
            let mut seen = Invocations::default();
            for event in events {
                assert!(event.sequence > 0 && event.sequence <= expected.total());
                assert!(sequences.insert(event.sequence), "duplicate event sequence");
                let counts = &mut seen.0[event.operation as usize];
                counts.completed += 1;
                if let Some(error) = event.error {
                    counts.failed += 1;
                    let required = match event.operation {
                        Operation::Insert => ErrorCode::SchemaMismatch,
                        Operation::Replace | Operation::Read => ErrorCode::StaleHandle,
                        Operation::Delete => ErrorCode::RowNotFound,
                        Operation::Snapshot => ErrorCode::QuotaExceeded,
                        Operation::Lookup => ErrorCode::TableNotFound,
                        Operation::NextPage => ErrorCode::LimitExceeded,
                        operation => panic!("unexpected error event for {operation:?}"),
                    };
                    assert_eq!(error, required);
                }
            }
            for (observed, required) in seen.0.iter().zip(expected.0.iter()) {
                assert!(observed.failed <= required.failed);
                assert!(
                    observed.completed - observed.failed <= required.completed - required.failed
                );
            }
        }
    }
}

struct RoundCounts {
    reader_operations: [u64; READERS],
    writer_operations: u64,
}

fn run_round(seed: u64, options: DiagnosticsOptions) -> RoundCounts {
    let mut store = VolatileStore::new_with_diagnostics(
        DatabaseId::new([7; 16]).unwrap(),
        1,
        1,
        StoreOptions {
            snapshot_pins: 5,
            ..StoreOptions::default()
        },
        options,
    )
    .unwrap();
    let mut counts = Invocations::default();
    // A dropped catalog retains its one-slot Vec capacity. Match that empty
    // state before/after the workload, rather than comparing it with the newly
    // constructed zero-capacity root. These calls are included in expectations.
    let capacity_fixture = counts.success(Operation::Create, store.create_table(schema()));
    counts.success(Operation::Drop, store.drop_table(&capacity_fixture));
    drop(capacity_fixture);
    let empty_bytes = store.metrics().resident_bytes;
    let table = counts.success(Operation::Create, store.create_table(schema()));
    assert_eq!(table.identity().object.get(), 0x1_0000_0002);
    let ids = std::array::from_fn(|row| {
        let id = counts.success(
            Operation::Insert,
            store.insert(&table, vec![Value::Int64(original(seed, row))]),
        );
        assert_eq!(id.row_id().get(), 0x1_0000_0001 + row as u64);
        id
    });
    counts.failure(
        Operation::Insert,
        store.insert(&table, vec![Value::Bool(true)]),
        ErrorCode::SchemaMismatch,
    );
    let barrier = Arc::new(Barrier::new(READERS + 1));
    let readers: Vec<_> = (0..READERS)
        .map(|index| {
            // Four acquisitions, not four clones of one pin.
            let snapshot = counts.success(Operation::Snapshot, store.snapshot());
            let table = table.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || reader(index, seed, snapshot, table, ids, barrier))
        })
        .collect();
    let mut events = Vec::new();
    let mut recreated = None;
    for phase in 0..PHASES {
        barrier.wait();
        match phase {
            0 => {
                for (row, id) in ids.iter().enumerate() {
                    counts.success(
                        Operation::Replace,
                        store.replace(&table, *id, vec![Value::Int64(-original(seed, row) - 1)]),
                    );
                    drain(&store, &mut events);
                    thread::yield_now();
                }
            }
            1 => {
                for row in (0..ROWS).step_by(2) {
                    counts.success(Operation::Delete, store.delete(&table, ids[row]));
                    drain(&store, &mut events);
                    thread::yield_now();
                }
                counts.failure(
                    Operation::Delete,
                    store.delete(&table, ids[0]),
                    ErrorCode::RowNotFound,
                );
            }
            2 => {
                counts.success(Operation::Drop, store.drop_table(&table));
                let fresh = counts.success(Operation::Create, store.create_table(schema()));
                assert_ne!(fresh.identity(), table.identity());
                assert_eq!(fresh.identity().object.get(), 0x1_0000_0003);
                let id = counts.success(
                    Operation::Insert,
                    store.insert(&fresh, vec![Value::Int64(99_999)]),
                );
                assert_eq!(id.row_id().get(), 0x1_0000_0011);
                counts.failure(
                    Operation::Replace,
                    store.replace(&table, ids[0], vec![Value::Int64(0)]),
                    ErrorCode::StaleHandle,
                );
                recreated = Some((fresh, id));
            }
            _ => unreachable!(),
        }
        let snapshot = counts.success(Operation::Snapshot, store.snapshot());
        counts.failure(
            Operation::Snapshot,
            store.snapshot(),
            ErrorCode::QuotaExceeded,
        );
        assert_eq!(store.metrics().snapshot_pins, 5);
        if let Some((fresh, id)) = &recreated {
            assert_eq!(
                counts.success(Operation::Lookup, snapshot.lookup_table("rows")),
                *fresh
            );
            let view = counts
                .success(Operation::Read, snapshot.read(fresh, *id))
                .unwrap();
            check_view(&view, *id, 99_999);
            counts.failure(
                Operation::Read,
                snapshot.read(&table, ids[0]),
                ErrorCode::StaleHandle,
            );
            counts.failure(
                Operation::Read,
                snapshot.read(fresh, ids[0]),
                ErrorCode::StaleHandle,
            );
        } else {
            for (row, id) in ids.iter().enumerate() {
                let view = counts.success(Operation::Read, snapshot.read(&table, *id));
                if phase == 1 && row.is_multiple_of(2) {
                    assert!(view.is_none());
                } else {
                    check_view(&view.unwrap(), *id, -original(seed, row) - 1);
                }
            }
        }
        drop(snapshot);
        drain(&store, &mut events);
        barrier.wait();
    }

    // Paired barriers keep each reader in the asserted ownership stage until the
    // writer observes it. Without the second barrier a fast reader could drop its
    // final view before the writer's pin assertion.
    barrier.wait();
    assert_eq!(store.metrics().snapshot_pins, READERS as u64);
    let page_and_view_bytes = store.metrics().resident_bytes;
    barrier.wait();
    barrier.wait();
    assert_eq!(store.metrics().snapshot_pins, READERS as u64);
    let view_bytes = store.metrics().resident_bytes;
    assert!(
        view_bytes < page_and_view_bytes,
        "page charges were not returned"
    );
    barrier.wait();
    barrier.wait();
    assert_eq!(store.metrics().snapshot_pins, 0);
    assert!(
        store.metrics().resident_bytes < view_bytes,
        "last view retained its charge"
    );
    barrier.wait();

    let mut reader_operations = [0; READERS];
    for (index, handle) in readers.into_iter().enumerate() {
        let reader_counts = handle.join().expect("reader panicked");
        reader_operations[index] = reader_counts.total();
        counts.merge(&reader_counts);
    }
    let (fresh, _) = recreated.unwrap();
    counts.success(Operation::Drop, store.drop_table(&fresh));
    assert_eq!(store.metrics().snapshot_pins, 0);
    assert_eq!(store.metrics().resident_bytes, empty_bytes);
    let writer_operations = counts.total() - reader_operations.iter().sum::<u64>();

    // All readers have joined. Only this final quiescent cut may use exact
    // counter and dropped-event conservation assertions.
    let final_drain = store.drain_diagnostics().unwrap();
    assert!(final_drain.available);
    assert!(final_drain.events.len() <= options.event_capacity);
    events.extend(final_drain.events);
    verify_diagnostics(&store, options, &counts, &events);
    RoundCounts {
        reader_operations,
        writer_operations,
    }
}

fn run_profile(minimum: Duration, seed_count: usize) {
    assert!((1..=256).contains(&seed_count));
    println!(
        "XADB_V3_PROFILE {{\"target\":\"{TARGET}\",\"generator\":\"{GENERATOR}\",\"seed\":0,\"seed_count\":{seed_count},\"seed_order\":\"0..seed_count-1 repeated, each with all five lanes\",\"min_seconds\":{},\"min_rounds\":{},\"max_rounds\":{MAX_ROUNDS},\"readers\":{READERS},\"writers\":1,\"rows\":{ROWS},\"phases\":{PHASES},\"reader_passes\":{READER_PASSES}}}",
        minimum.as_secs(),
        seed_count * LANES.len()
    );
    // A real dedicated writer thread owns all mutations, with four actual reader
    // threads per round. No mutex serializes the reader API behind the writer.
    thread::spawn(move || {
        let started = Instant::now();
        let mut rounds = 0;
        let mut lane_rounds = [0_u64; LANES.len()];
        let mut visited = [[false; 256]; LANES.len()];
        let mut reader_operations = [0_u64; READERS];
        let mut writer_operations = 0;
        while started.elapsed() < minimum || rounds < (seed_count * LANES.len()) as u64 {
            assert!(rounds < MAX_ROUNDS, "round cap reached before profile completed");
            let lane = rounds as usize % LANES.len();
            let seed = (rounds as usize / LANES.len()) % seed_count;
            let counts = run_round(seed as u64, LANES[lane]);
            for (total, count) in reader_operations.iter_mut().zip(counts.reader_operations) {
                *total += count;
            }
            writer_operations += counts.writer_operations;
            lane_rounds[lane] += 1;
            visited[lane][seed] = true;
            rounds += 1;
        }
        let elapsed = started.elapsed();
        let visited_combinations: usize = visited
            .iter()
            .map(|lane| lane[..seed_count].iter().filter(|seen| **seen).count())
            .sum();
        let missing_lanes = lane_rounds.iter().filter(|count| **count == 0).count();
        assert_eq!(missing_lanes, 0);
        assert_eq!(visited_combinations, LANES.len() * seed_count);
        assert!(lane_rounds.iter().all(|count| *count >= seed_count as u64));
        assert!(reader_operations.iter().all(|count| *count > 0));
        assert!(writer_operations > 0);
        assert!(elapsed >= minimum);
        println!(
            "XADB_V3_RESULT {{\"target\":\"{TARGET}\",\"generator\":\"{GENERATOR}\",\"seed\":0,\"seed_count\":{seed_count},\"elapsed_ms\":{},\"rounds\":{rounds},\"lane_rounds\":{{\"off\":{},\"always\":{},\"detailed_0\":{},\"detailed_1\":{},\"detailed_256\":{}}},\"reader_operations\":{},\"reader_operations_by_thread\":{:?},\"writer_operations\":{writer_operations},\"visited_seed_lanes\":{visited_combinations},\"missing_lanes\":{missing_lanes}}}",
            elapsed.as_millis(),
            lane_rounds[0], lane_rounds[1], lane_rounds[2], lane_rounds[3], lane_rounds[4],
            reader_operations.iter().sum::<u64>(), reader_operations
        );
    })
    .join()
    .expect("writer panicked");
}

#[test]
fn four_readers_keep_old_rows_and_release_pins_smoke() {
    run_profile(Duration::ZERO, 2);
}

#[test]
#[ignore = "V3 requires at least 30 minutes; run with the retained-binary evidence wrapper"]
fn four_readers_keep_old_rows_and_release_pins_v3_30_minutes() {
    run_profile(Duration::from_secs(1_800), 256);
}
