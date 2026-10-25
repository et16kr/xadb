//! Selected explorer cases on small profiles; each run uses the reference model and checker
//! unless a mutant model hook is named.
use super::*;
use std::path::PathBuf;
use test_reference::lifecycle::{Phase, Retry};

fn unhex(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex"))
        .collect()
}

fn initial() -> Box<State> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tests/fixtures/dev4-lifecycle-model/vectors/state-initial.bin");
    let mut state = Box::<State>::default();
    load_initial(&path, &mut state).expect("independent initial vector");
    state
}

fn small(max_depth: u8, workers: usize) -> Profile {
    Profile {
        max_depth,
        workers,
        ..Profile::normal(workers)
    }
}

fn run(profile: &Profile, hooks: Hooks) -> Report {
    let initial = initial();
    let profile = *profile;
    std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(move || explore(&profile, hooks, &initial))
        .expect("spawn")
        .join()
        .expect("explorer")
}

fn relations(report: &Report) {
    let t = report.totals;
    assert_eq!(t.checker, t.success + t.rejection + t.excluded);
    assert_eq!(report.edges, t.success + t.rejection + t.alias + t.excluded);
    let mut api = Counts::default();
    for c in &report.per_api {
        api.success += c.success;
        api.rejection += c.rejection;
        api.alias += c.alias;
        api.excluded += c.excluded;
        api.checker += c.checker;
    }
    assert_eq!(api, t);
    let unique: u64 = report.depths.iter().map(|d| d.unique).sum();
    assert_eq!(unique, u64::from(report.states));
    for d in &report.depths {
        assert_eq!(d.invariant, d.unique);
    }
}

#[test]
fn depth_one_classifies_every_slot_of_initial() {
    let report = run(&small(1, 2), Hooks::REFERENCE);
    assert_eq!(report.status, Status::DrainedUnarchived, "{report:?}");
    assert_eq!(report.edges, u64::from(RAW_ACTION_COUNT));
    assert_eq!(report.depths[0].expanded, 1);
    assert_eq!(
        report.depths[1].expanded, 0,
        "maximum-depth states are not expanded"
    );
    assert!(report.depths[1].unique > 0);
    relations(&report);
    println!(
        "EXPLORER depth1 states={} totals={:?} ledger={:?}",
        report.states, report.totals, report.ledger_sha256
    );
}

#[test]
fn worker_count_does_not_change_results() {
    let one = run(&small(2, 1), Hooks::REFERENCE);
    let many = run(&small(2, 6), Hooks::REFERENCE);
    assert_eq!(one.status, Status::DrainedUnarchived);
    assert_eq!(one.ledger_sha256, many.ledger_sha256);
    assert_eq!(one.discovery_sha256, many.discovery_sha256);
    assert_eq!(one.totals, many.totals);
    assert_eq!(one.depths, many.depths);
    assert_eq!(one.per_api, many.per_api);
    relations(&one);
    assert_eq!(
        one.edges,
        u64::from(RAW_ACTION_COUNT) * (one.depths[0].expanded + one.depths[1].expanded)
    );
    println!(
        "EXPLORER depth2 states={} edges={} ledger={:?}",
        one.states, one.edges, one.ledger_sha256
    );
}

#[test]
fn state_cap_stops_at_the_exact_state() {
    let profile = Profile {
        state_cap: 100,
        ..small(3, 3)
    };
    let report = run(&profile, Hooks::REFERENCE);
    assert_eq!(report.status, Status::Inconclusive);
    assert_eq!(report.stop, Stop::StateCap);
    assert_eq!(report.states, 100);
    let frontier = report.frontier;
    let current = frontier.current.expect("stopped inside a state");
    assert!(current >= frontier.head && current < frontier.tail);
    relations(&report);
    // The 101st discovery was refused: every published state is still counted once.
    assert_eq!(report.depths.iter().map(|d| d.unique).sum::<u64>(), 100);
}

#[test]
fn slot_cap_stops_before_the_next_slot() {
    let profile = Profile {
        slot_cap: 1_000,
        ..small(2, 2)
    };
    let report = run(&profile, Hooks::REFERENCE);
    assert_eq!(report.stop, Stop::SlotCap);
    assert_eq!(report.edges, 1_000);
    assert_eq!(report.frontier.next_raw, 1_000);
    relations(&report);
}

/// Mutant: a StalePlan entry rejection claims an operation retry it cannot have.
fn mutant_model(
    pre: &State,
    input: &ResolvedInput,
    post: &mut State,
    edge: &mut Edge,
) -> Result<(), ModelError> {
    transition(pre, input, post, edge)?;
    if let Some(response) = edge.response.as_mut()
        && response.code == ReturnCode::StalePlan
        && response.phase == Phase::Entry
    {
        response.retry = Retry::Operation;
        if let Some(d) = response.diagnostic.as_mut() {
            d.retry = Retry::Operation;
        }
    }
    Ok(())
}

#[test]
fn mutant_model_fails_with_its_counterexample() {
    let hooks = Hooks {
        model: mutant_model,
        check: verify_edge,
    };
    let profile = small(3, 4);
    let report = run(&profile, hooks);
    assert_eq!(report.status, Status::Fail, "{:?}", report.stop);
    assert_eq!(report.stop, Stop::Counterexample);
    let c = report.counterexample.clone().expect("counterexample");
    assert!(c.edge.contains("StalePlan"), "{}", c.edge);
    assert!(!c.pre.is_empty() && !c.input.is_empty());
    assert_eq!(report.frontier.current, Some(c.state));
    assert_eq!(report.frontier.next_raw, c.raw);
    assert_eq!(
        c.path.len(),
        usize::from(c.depth),
        "path from initial to the pre-state"
    );
    assert!(!c.post.is_empty());
    relations(&report);
    let normal = run(&profile, Hooks::REFERENCE);
    assert_eq!(normal.status, Status::DrainedUnarchived);
}

/// Profile run, selected only by `XADB_BFS_RUN=<report path>`; `XADB_BFS_WORKERS` sets the worker
/// count and `XADB_BFS_PROFILE=depth5` the approved depth-5 extension. The runner preserves
/// sources and the executable around it.
#[test]
fn profile_run() {
    let Ok(path) = std::env::var("XADB_BFS_RUN") else {
        return;
    };
    let workers = std::env::var("XADB_BFS_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8);
    let profile = match std::env::var("XADB_BFS_PROFILE").as_deref() {
        Ok("depth5") => Profile::depth_five(workers),
        _ => Profile::normal(workers),
    };
    let report = run(&profile, Hooks::REFERENCE);
    let json = serde_json::to_string_pretty(&report.to_json(&profile)).expect("json");
    std::fs::write(&path, json + "\n").expect("write report");
    println!(
        "EXPLORER normal status={:?} stop={:?} states={} edges={} elapsed={:?}",
        report.status, report.stop, report.states, report.edges, report.elapsed
    );
}

#[test]
fn wide_ledger_keeps_counts_and_discovery() {
    let narrow = run(&small(2, 2), Hooks::REFERENCE);
    let wide = run(
        &Profile {
            state_cap: 1 << 19,
            ..small(2, 2)
        },
        Hooks::REFERENCE,
    );
    assert!(wide.wide_ledger && !narrow.wide_ledger);
    assert_eq!(wide.discovery_sha256, narrow.discovery_sha256);
    assert_ne!(wide.ledger_sha256, narrow.ledger_sha256);
    assert_eq!(wide.totals, narrow.totals);
    assert_eq!(wide.depths, narrow.depths);
}

/// Literal vectors copied from checkpoint-wire-v1.json `byte_vectors` (ids in the comments);
/// the expected bytes are the independent fixture's, not this encoder's output.
#[test]
fn edge8_and_discovery32_match_the_checkpoint_vectors() {
    let cases: [(&str, u32, u16, u8, u16, u64, bool); 6] = [
        ("0000000000100080", 0, 0, 0, 0, 2, true), // edge-first-success
        ("0700e02f00400000", 7, 1532, 0, 0, 8, false), // edge-self-success
        ("030088401c500080", 3, 17, 1, 28, 10, true), // edge-context-rejection
        ("000070800c080000", 0, 14, 2, 12, 1, false), // edge-alias
        ("0000f8c001000000", 0, 31, 3, 1, 0, false), // edge-exclusion
        ("0000606a1c080000", 0, 1356, 1, 28, 1, false), // edge-signal-invalid-context-fixture
    ];
    for (hex, pre, raw, class, detail, succ, first) in cases {
        let word = edge8(pre, raw, class, detail, succ, first);
        assert_eq!(word.to_le_bytes().to_vec(), unhex(hex), "{hex}");
    }
    // discovery-genesis: the initial frame with its 444-byte independent wire.
    let frame = discovery32(0, u32::MAX, 0, u16::MAX, u64::MAX, 444);
    assert_eq!(
        frame.to_vec(),
        unhex("00000000ffffffff00000000ffff0000ffffffffffffffffbc01000000000000")
    );
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tests/fixtures/dev4-lifecycle-model/vectors/state-initial.bin");
    let vector = std::fs::read(path).expect("initial vector");
    let mut out = vec![0u8; MAX_STATE_BYTES];
    let n = encode_state(&initial(), &vector_identity(), &mut out).expect("encode initial");
    assert_eq!(
        out[..n].to_vec(),
        vector,
        "canonical initial equals the independent vector"
    );
}

fn permissive(_: &State, _: &ResolvedInput, _: &Edge, _: &State) -> Result<(), CheckFailure> {
    Ok(())
}

fn model_error(
    _: &State,
    _: &ResolvedInput,
    _: &mut State,
    _: &mut Edge,
) -> Result<(), ModelError> {
    Err(ModelError::Capacity)
}

fn envelope_everywhere(
    pre: &State,
    _: &ResolvedInput,
    post: &mut State,
    edge: &mut Edge,
) -> Result<(), ModelError> {
    post.copy_from(pre);
    *edge = Edge {
        class: EdgeClass::Excluded(Exclusion::Envelope),
        ..Edge::default()
    };
    Ok(())
}

fn no_response(
    pre: &State,
    input: &ResolvedInput,
    post: &mut State,
    edge: &mut Edge,
) -> Result<(), ModelError> {
    transition(pre, input, post, edge)?;
    if !matches!(edge.class, EdgeClass::Excluded(_)) {
        edge.response = None;
    }
    Ok(())
}

#[test]
fn every_fail_cause_stops_with_its_reason() {
    let cases: [(ModelFn, CheckFn, Stop); 3] = [
        (model_error, verify_edge, Stop::ModelError),
        (envelope_everywhere, permissive, Stop::EnvelopeExclusion),
        (no_response, permissive, Stop::ModelError),
    ];
    for (model, check, stop) in cases {
        let report = run(&small(1, 2), Hooks { model, check });
        assert_eq!(report.status, Status::Fail);
        assert_eq!(report.stop, stop);
        let c = report.counterexample.clone().expect("counterexample");
        assert_eq!(
            (c.state, c.raw),
            (0, 0),
            "the first slot of the initial state"
        );
        assert!(c.path.is_empty());
        relations(&report);
    }
}

#[test]
fn wall_and_capacity_caps_are_inconclusive() {
    let wall = run(
        &Profile {
            wall_cap: Duration::ZERO,
            ..small(2, 2)
        },
        Hooks::REFERENCE,
    );
    assert_eq!(
        (wall.status, wall.stop),
        (Status::Inconclusive, Stop::WallCap)
    );
    assert_eq!(wall.edges, 0);
    let capacity = run(
        &Profile {
            capacity_cap: WORKER_ALLOWANCE * 2 + 1024,
            ..small(2, 2)
        },
        Hooks::REFERENCE,
    );
    assert_eq!(
        (capacity.status, capacity.stop),
        (Status::Inconclusive, Stop::CapacityCap)
    );
}

#[test]
fn aliases_cost_no_checker_call_and_raw_totals_close() {
    let report = run(&small(2, 3), Hooks::REFERENCE);
    assert!(
        report.totals.alias > 0,
        "the initial state has equal resolved inputs"
    );
    assert_eq!(report.totals.checker, report.edges - report.totals.alias);
    let expanded: u64 = report.depths.iter().map(|d| d.expanded).sum();
    assert_eq!(report.edges, expanded * u64::from(RAW_ACTION_COUNT));
    assert_eq!(report.per_rule.iter().sum::<u64>(), report.totals.excluded);
}

/// Replays the counterexample of `XADB_BFS_REPLAY=<report path>` from its exact pre-state and
/// input bytes and prints the observation and the pre-state.
#[test]
fn counterexample_replay() {
    let Ok(path) = std::env::var("XADB_BFS_REPLAY") else {
        return;
    };
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).expect("report")).expect("json");
    let c = &report["counterexample"];
    let pre_bytes = unhex(c["pre_hex"].as_str().expect("pre"));
    let input_bytes = unhex(c["input_hex"].as_str().expect("input"));
    std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(move || {
            let mut pre = Box::<State>::default();
            decode_state(&pre_bytes, &vector_identity(), &mut pre).expect("pre");
            let mut input = Box::<ResolvedInput>::default();
            test_reference::lifecycle::decode_input(&input_bytes, &mut input).expect("input");
            let mut post = Box::<State>::default();
            let mut edge = Edge::default();
            transition(&pre, &input, &mut post, &mut edge).expect("model");
            println!("INPUT {input:#?}");
            println!("EDGE {edge:#?}");
            println!("VERDICT {:?}", verify_edge(&pre, &input, &edge, &post));
            println!("PRE {pre:#?}");
        })
        .expect("spawn")
        .join()
        .expect("replay");
}
