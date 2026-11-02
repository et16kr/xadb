//! F6 M0 (evidence `f6-wal-revalidation-survey-20261102.md` §7, stage M0):
//! the full WAL revalidations (`replay_durable`) of a one-row Row
//! transaction and of its live cleanup, counted per step and per entry at
//! the WAL lengths L0–L3 of one fresh Genesis epoch, without reopening.
//! Between levels a build transaction grows the WAL by Reserve→Cancel
//! cycles and a full rollback, and its Terminal is cleaned live. No time is
//! measured; the counts are deterministic.
//!
//! The counts themselves are the measured result. The checks that can fail
//! on their own are contract facts: reserve, prepare, apply and cancel do not
//! replay (S2W-R22); Begin, the write preflight, Commit and each cleanup
//! group replay at least once (§4); the replays of Begin and of the write
//! preflight read up to the whole WAL file as it stands; two runs of the same
//! input give the same counts. The §3 identities (3E read, 2E hashed, one
//! re-applied frame per LSN, every replay named) only confirm that the
//! counters sit where §3 says: a replay that broke them would already have
//! been refused by the product's own end-offset and LSN checks.
use super::super::super::super::super::super::plan4_pages::apply::test_revalidations;
use super::*;

/// The WAL file of the fresh directory.
const WAL: &str = "d3-wal-00000001";
/// L1–L3 (§7.3): the WAL length each level's first measured transaction
/// starts at or beyond; a level the WAL already passed is measured at once,
/// without a build. L0 is the length after the warmup; L4 and L5 are left to
/// M1 (§7.2).
const LEVELS: [(&str, usize); 3] = [
    ("L1", 1_048_576),
    ("L2", 4 * 1_048_576),
    ("L3", 16 * 1_048_576),
];
/// Measured one-row transactions per level (§7.3).
const MEASURED: usize = 3;
/// Pool frames: the cleanup needs eleven or more (LRR-06); the live Row
/// retirement cases use sixteen.
const FRAMES: usize = 16;

/// One interval's counts as a JSON object.
macro_rules! count_json {
    ($name:expr, $count:expr) => {{
        let count = $count;
        format!(
            "{{\"step\":\"{}\",\"live\":{},\"lookup\":{},\"capture\":{},\"seeds\":{},\
             \"ends\":{},\"last_end\":{},\"lsns\":{},\"read\":{},\"hashed\":{},\"frames\":{}}}",
            $name,
            count.replays[0],
            count.replays[1],
            count.replays[2],
            count.seeds,
            count.ends,
            count.last_end,
            count.lsns,
            count.read,
            count.hashed,
            count.frames
        )
    }};
}

/// The §3 identities over an interval: every replay is named by its entry,
/// reads 3E and hashes 2E, and re-applies the frames E holds. They confirm
/// the counters' placement, not an independent measurement.
macro_rules! formulas {
    ($name:expr, $count:expr) => {{
        let count = $count;
        assert_eq!(count.replays[3], 0, "{}: a replay without an entry", $name);
        assert_eq!(count.read, 3 * count.ends, "{}: read is not 3E", $name);
        assert_eq!(count.hashed, 2 * count.ends, "{}: hashed is not 2E", $name);
        assert_eq!(
            count.frames, count.lsns,
            "{}: frames are not the LSNs",
            $name
        );
    }};
}

/// Records the counts since the last mark as step `name` and moves the mark.
macro_rules! mark {
    ($steps:ident, $mark:ident, $name:expr) => {{
        let name = $name;
        let now = test_revalidations();
        let interval = now.since($mark);
        formulas!(name, interval);
        $steps.push((name.to_string(), interval));
        #[allow(
            unused_assignments,
            reason = "the last step of a list moves the mark too"
        )]
        {
            $mark = now;
        }
    }};
}

/// The current WAL file length.
fn wal(state: &State) -> usize {
    state.lock().unwrap().files[WAL].bytes.len()
}

/// (Free, Row-scope held, other held) Job cells.
fn occupancy(jobs: &[[u8; 1024]; 64]) -> (usize, usize, usize) {
    let mut cells = (0, 0, 0);
    for bytes in jobs {
        let job =
            job3::decode_job(bytes, super::super::super::super::support::namespace()).unwrap();
        match (job.state(), job.scope()) {
            (JobState::Free, _) => cells.0 += 1,
            (_, Some(JobScope::Row)) => cells.1 += 1,
            _ => cells.2 += 1,
        }
    }
    cells
}

/// One M0 run in a fresh directory: the warmup transaction, L0 and each
/// level of `levels`, `measured` transactions at each. Returns the result
/// lines (JSON), printed as they are made.
fn m0_run(levels: &[(&str, usize)], measured: usize) -> Vec<String> {
    let oracle = Oracle::new();
    with_fresh!(
        oracle,
        frames = FRAMES,
        file_bytes = 64 * 1_048_576,
        |live, owner, flusher, status, undo, private, reader_private, _core, fence, state| {
            let (row_status, _) = create_table(live, owner, flusher, status, undo, private, true);
            let (mut writer, reader, mut flush) = live.split();
            let mut lines = Vec::new();
            let mut emit = |line: String| {
                println!("F6_M0 {line}");
                lines.push(line);
            };
            let free = occupancy(writer.catalog_test_jobs());
            emit(format!(
                "{{\"kind\":\"start\",\"frames\":{FRAMES},\"wal\":{},\"jobs_free\":{},\
                 \"jobs_row\":{},\"jobs_other\":{}}}",
                wal(state),
                free.0,
                free.1,
                free.2
            ));
            let mut value = 0i64;

            // BEGIN → INSERT one row → COMMIT, then its live cleanup; every
            // step's counts. Returns the step list and the actor's Q.
            macro_rules! one_row {
                () => {{
                    value += 1;
                    let mut steps = Vec::new();
                    let mut mark = test_revalidations();
                    let start = wal(state) as u64;
                    let tx = writer
                        .begin_row_transaction(
                            owner,
                            IsolationLevel::Serializable,
                            &row_status,
                            private,
                            &mut flush,
                        )
                        .unwrap();
                    mark!(steps, mark, "begin");
                    // H1: the Begin group replays the whole WAL as it stood.
                    assert_eq!(steps[0].1.last_end, start, "Begin replays the whole WAL");
                    let view = writer.begin_statement(owner, &tx).unwrap();
                    mark!(steps, mark, "statement");
                    let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                    mark!(steps, mark, "table");
                    let mut flushed = 0;
                    loop {
                        match writer
                            .preflight_row(owner, &view, &table, private, &mut flush)
                            .unwrap()
                        {
                            RowPreflightStep::Ready(mut preflight) => {
                                mark!(steps, mark, "preflight");
                                // H1: the preflight writes no frame and
                                // replays the whole WAL as it stands.
                                assert_eq!(
                                    steps.last().unwrap().1.last_end,
                                    wal(state) as u64,
                                    "the preflight replays the whole WAL"
                                );
                                let values = [ValueRef::Int64(value)];
                                let input =
                                    validate_insert(&values, preflight.validation_lane().unwrap())
                                        .unwrap();
                                mark!(steps, mark, "validate");
                                let mut reserved = applied!(preflight.reserve(input));
                                mark!(steps, mark, "reserve");
                                let proposal =
                                    produce_insert(reserved.producer_lane().unwrap()).unwrap();
                                mark!(steps, mark, "produce");
                                let prepared = applied!(reserved.prepare(proposal));
                                mark!(steps, mark, "prepare");
                                applied!(prepared.apply());
                                mark!(steps, mark, "apply");
                                break;
                            }
                            RowPreflightStep::FlushRequired { target } => {
                                mark!(steps, mark, "preflight_flush_required");
                                flushed += 1;
                                assert!(flushed < 16, "bounded preflight residency");
                                flush_target!(flush, flusher, target);
                                mark!(steps, mark, "preflight_flush");
                            }
                        }
                    }
                    assert!(writer.finish_statement(owner, view).is_ok());
                    mark!(steps, mark, "finish_statement");
                    let q = committed!(writer.commit_row(owner, tx, private)).q();
                    mark!(steps, mark, "commit");
                    let mut done = Vec::new();
                    while let Some((ordinal, operation)) = writer.row_cleanup_next(&[]).unwrap() {
                        mark!(steps, mark, "cleanup_next");
                        loop {
                            match writer.retire_row(owner, ordinal, operation, private, &mut flush)
                            {
                                Ok(RowRetireStep::Retired(_)) => break,
                                Ok(RowRetireStep::FlushRequired(target)) => {
                                    mark!(steps, mark, format!("{operation:?}_flush_required"));
                                    flush_target!(flush, flusher, target);
                                    mark!(steps, mark, format!("{operation:?}_flush"));
                                }
                                Err(error) => panic!("live {operation:?}: {error:?}"),
                            }
                        }
                        mark!(steps, mark, format!("{operation:?}"));
                        done.push(operation);
                        assert!(done.len() <= COMMITTED.len(), "bounded live cleanup");
                    }
                    assert_eq!(done, COMMITTED);
                    assert_row_jobs_free(writer.catalog_test_jobs());
                    assert!(!fence.is_fenced());
                    (steps, q)
                }};
            }

            // One measured transaction's line, with the WAL length it
            // starts at.
            macro_rules! measured_line {
                ($level:expr, $txn:expr) => {{
                    let at = wal(state);
                    let (steps, q) = one_row!();
                    let body: Vec<String> = steps
                        .iter()
                        .map(|(name, count)| count_json!(name, count))
                        .collect();
                    format!(
                        "{{\"kind\":\"txn\",\"level\":\"{}\",\"txn\":{},\"wal\":{},\"q\":{q},\
                         \"steps\":[{}]}}",
                        $level,
                        $txn,
                        at,
                        body.join(",")
                    )
                }};
            }

            // BEGIN → (Reserve → Cancel) until the WAL reaches `target` →
            // full rollback, then the live TerminalFinish (§7.3 build).
            macro_rules! build {
                ($level:expr, $target:expr) => {{
                    let before = wal(state);
                    let mut steps = Vec::new();
                    let start = test_revalidations();
                    let mut mark = start;
                    let tx = writer
                        .begin_row_transaction(
                            owner,
                            IsolationLevel::Serializable,
                            &row_status,
                            private,
                            &mut flush,
                        )
                        .unwrap();
                    let view = writer.begin_statement(owner, &tx).unwrap();
                    let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                    mark!(steps, mark, "begin");
                    let (mut cycles, mut flushes) = (0usize, 0usize);
                    let mut first = None;
                    while wal(state) < $target {
                        loop {
                            match writer
                                .preflight_row(owner, &view, &table, private, &mut flush)
                                .unwrap()
                            {
                                RowPreflightStep::Ready(mut preflight) => {
                                    let values = [ValueRef::Int64(0)];
                                    let input = validate_insert(
                                        &values,
                                        preflight.validation_lane().unwrap(),
                                    )
                                    .unwrap();
                                    let reserved = applied!(preflight.reserve(input));
                                    if let Err(error) = reserved.cancel() {
                                        let (RowOwnedError::Before { error, .. }
                                        | RowOwnedError::Fatal(error)) = error;
                                        panic!(
                                            "{} cancel {cycles} at wal {}: {error:?}",
                                            $level,
                                            wal(state)
                                        );
                                    }
                                    break;
                                }
                                RowPreflightStep::FlushRequired { target } => {
                                    flushes += 1;
                                    assert!(flushes < 16 * (cycles + 1), "bounded residency");
                                    flush_target!(flush, flusher, target);
                                }
                            }
                        }
                        cycles += 1;
                        if first.is_none() {
                            first = Some(test_revalidations().since(mark));
                        }
                    }
                    mark!(steps, mark, "cycles");
                    assert!(writer.finish_statement(owner, view).is_ok());
                    {
                        let mut rollback = writer
                            .start_row_rollback(
                                owner,
                                &tx,
                                None,
                                RollbackScope::Transaction,
                                private,
                                &mut flush,
                                flusher,
                            )
                            .unwrap();
                        loop {
                            match rollback.next().unwrap() {
                                RollbackStep::Candidate(_) => panic!("no applied row to undo"),
                                RollbackStep::FlushRequired { target } => {
                                    rollback.flush(&target).unwrap()
                                }
                                RollbackStep::Complete => break,
                            }
                        }
                        applied!(rollback.finish());
                    }
                    assert!(writer.finish_aborted(owner, tx).is_ok());
                    mark!(steps, mark, "rollback");
                    let done = retire_all!(writer, owner, private, flush, flusher);
                    assert_eq!(done, [RowCleanup::TerminalFinish]);
                    mark!(steps, mark, "cleanup");
                    assert_row_jobs_free(writer.catalog_test_jobs());
                    assert!(!fence.is_fenced());
                    let body: Vec<String> = first
                        .iter()
                        .map(|count| count_json!("first_cycle", count))
                        .chain(steps.iter().map(|(name, count)| count_json!(name, count)))
                        .chain(std::iter::once(count_json!(
                            "total",
                            test_revalidations().since(start)
                        )))
                        .collect();
                    format!(
                        "{{\"kind\":\"build\",\"level\":\"{}\",\"target\":{},\"wal_before\":{before},\
                         \"wal_after\":{},\"cycles\":{cycles},\"flushes\":{flushes},\
                         \"steps\":[{}]}}",
                        $level,
                        $target,
                        wal(state),
                        body.join(",")
                    )
                }};
            }

            // The level's start: WAL length and Job occupancy.
            macro_rules! level_line {
                ($level:expr) => {{
                    let cells = occupancy(writer.catalog_test_jobs());
                    format!(
                        "{{\"kind\":\"level\",\"level\":\"{}\",\"wal\":{},\"jobs_free\":{},\
                         \"jobs_row\":{},\"jobs_other\":{}}}",
                        $level,
                        wal(state),
                        cells.0,
                        cells.1,
                        cells.2
                    )
                }};
            }

            let warmup = measured_line!("warmup", 0);
            emit(warmup);
            for (index, (level, target)) in std::iter::once(("L0", 0))
                .chain(levels.iter().copied())
                .enumerate()
            {
                // A level the measured transactions already passed needs
                // no build transaction.
                if index > 0 && wal(state) < target {
                    let line = build!(level, target);
                    emit(line);
                }
                let line = level_line!(level);
                emit(line);
                for txn in 0..measured {
                    let line = measured_line!(level, txn);
                    emit(line);
                }
            }
            let total = state
                .lock()
                .unwrap()
                .files
                .values()
                .map(|file| file.length)
                .sum::<u64>();
            emit(format!(
                "{{\"kind\":\"end\",\"wal\":{},\"file_bytes\":{total}}}",
                wal(state)
            ));
            lines
        }
    )
}

/// The (name, field) values of every step object in a result line.
fn steps_of(line: &str) -> Vec<(String, Vec<(String, u64)>)> {
    line.split("{\"step\":\"")
        .skip(1)
        .map(|object| {
            let object = &object[..object.find('}').unwrap()];
            let (name, rest) = object.split_once('"').unwrap();
            let fields = rest
                .split(',')
                .skip(1)
                .map(|pair| {
                    let (key, value) = pair.split_once(':').unwrap();
                    (key.trim_matches('"').to_string(), value.parse().unwrap())
                })
                .collect();
            (name.to_string(), fields)
        })
        .collect()
}

/// The full replays of every entry in one step's fields.
fn replays_of(fields: &[(String, u64)]) -> u64 {
    fields
        .iter()
        .filter(|(key, _)| matches!(key.as_str(), "live" | "lookup" | "capture"))
        .map(|(_, value)| value)
        .sum()
}

/// F6-M0-S: the counters over the warmup, one L0 transaction, a small build
/// transaction (to 2 MiB, a few Reserve→Cancel cycles) and one more
/// transaction, each with its live cleanup, twice in fresh directories. The
/// two runs give the same lines. In every measured transaction validate,
/// reserve, produce, prepare and apply replay nothing (S2W-R22), and Begin,
/// the write preflight, Commit and every cleanup group replay at least once
/// (§4); Begin and the preflight replay the whole WAL (checked as the steps
/// are recorded). In the build, every cycle replays once in its preflight.
///
/// Both runs revalidate the growing WAL in every group (about 20 s in a
/// debug build), so the case takes its own 60 s bound.
#[test]
fn f6_m0_step_counts_match_the_contract_and_repeat() {
    case_within("F6-M0-S", std::time::Duration::from_secs(60), || {
        let levels = [("S1", 2 * 1_048_576)];
        let first = m0_run(&levels, 1);
        let second = m0_run(&levels, 1);
        assert_eq!(first, second, "M0 counts are deterministic");
        let mut checked = 0;
        for line in first
            .iter()
            .filter(|line| line.contains("\"kind\":\"txn\""))
        {
            for (name, fields) in steps_of(line) {
                match name.as_str() {
                    "validate" | "reserve" | "produce" | "prepare" | "apply" => {
                        assert_eq!(replays_of(&fields), 0, "{name} replays nothing");
                    }
                    "begin" | "preflight" | "commit" | "Prune" | "ReleaseRole" | "RootFree"
                    | "TerminalFinish" => {
                        assert!(replays_of(&fields) >= 1, "{name} replays the WAL");
                        checked += 1;
                    }
                    _ => {}
                }
            }
        }
        // warmup, L0 and S1: three transactions of nine checked steps.
        assert_eq!(checked, 27);
        let build = first
            .iter()
            .find(|line| line.contains("\"kind\":\"build\""))
            .expect("the S1 build");
        let cycles: u64 = build
            .split("\"cycles\":")
            .nth(1)
            .and_then(|rest| rest.split(',').next())
            .unwrap()
            .parse()
            .unwrap();
        assert!(cycles >= 1);
        let steps = steps_of(build);
        let step = |wanted: &str| {
            let (_, fields) = steps.iter().find(|(name, _)| name == wanted).unwrap();
            fields.clone()
        };
        assert_eq!(replays_of(&step("cycles")), cycles, "one replay per cycle");
        assert!(replays_of(&step("rollback")) >= 1);
        assert!(replays_of(&step("cleanup")) >= 1);
        println!(
            "F6_M0_S runs=2 lines={} build_cycles={cycles} deterministic",
            first.len()
        );
    });
}

/// F6 M0 (§7.2–§7.3): warmup, L0 and L1–L3 with three measured one-row
/// transactions each and their live cleanup. Long (the build to 16 MiB
/// revalidates the growing WAL every cycle), so it runs only by its explicit
/// selector with `--ignored`; the evidence runs it twice and compares the
/// lines.
#[test]
#[ignore = "F6 M0 measurement: run by its explicit selector"]
fn f6_m0_levels_to_l3() {
    case_within("F6-M0", std::time::Duration::from_secs(1800), || {
        let lines = m0_run(&LEVELS, MEASURED);
        println!("F6_M0_DONE lines={}", lines.len());
    });
}
