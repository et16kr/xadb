//! Fixed vectors for the independent controller, not measurements of product speed.
use test_controller::performance::{
    CampaignKind, Exploration, Recipe, RunSample, Side, analyze, check_output,
};
use test_protocol::{
    engine::Metrics,
    performance::{
        BuildIdentity, DiagnosticsMode, DiagnosticsSnapshot, Event, Operation, OperationCounts,
        Output, Phase,
    },
};

fn side() -> Side {
    Side {
        worker_sha256: "a".repeat(64),
        receipt_sha256: "b".repeat(64),
        config_sha256: "c".repeat(64),
        build: BuildIdentity {
            source_sha256: "d".repeat(64),
            toolchain: "fixed vector".into(),
            target: "x86_64-unknown-linux-gnu".into(),
            profile: "release".into(),
            self_test: false,
        },
        mode: DiagnosticsMode::Off,
    }
}
fn recipe() -> Recipe {
    Recipe::instrumentation([7; 16], 7, side(), DiagnosticsMode::Always).unwrap()
}
fn output(recipe: &Recipe, candidate: bool) -> Output {
    let mut input = recipe.fixture.clone();
    input.mode = if candidate {
        recipe.candidate.mode
    } else {
        recipe.baseline.mode
    };
    let mut state = input.seed;
    let mut rows: Vec<_> = (0..input.row_count).map(i64::from).collect();
    let mut observed = Vec::new();
    for phase in [input.warmup, input.measure] {
        for iteration in 0..phase {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            rows[(iteration % input.row_count) as usize] = state as i64;
            if phase == input.measure {
                observed.push(state as i64);
            }
        }
    }
    let metrics = Metrics {
        operations: 0,
        rows_read: 0,
        row_references_copied: 0,
        resident_bytes: 4096,
        snapshot_pins: u64::from(input.pin_old),
    };
    let mut after = metrics.clone();
    after.operations = u64::from(input.measure);
    after.rows_read = u64::from(input.measure);
    // A decreasing resident gauge is valid and is never subtracted as a counter.
    after.resident_bytes = 1024;
    let collected = input.mode != DiagnosticsMode::Off;
    let detailed = input.mode == DiagnosticsMode::Detailed;
    let diagnostics = DiagnosticsSnapshot {
        mode: input.mode,
        event_capacity: input.capacity,
        logical_bytes: if collected { 512 } else { 0 },
        operations: collected.then_some([OperationCounts::default(); 11]),
        dropped_events: detailed.then_some(0),
        saturated: collected.then_some(false),
    };
    let mut diagnostics_after = diagnostics.clone();
    if let Some(counters) = &mut diagnostics_after.operations {
        for index in [3, 5, 8] {
            counters[index] = OperationCounts {
                started: u64::from(input.measure),
                completed: u64::from(input.measure),
                failed: 0,
            };
        }
    }
    let events = if detailed {
        (0..input.capacity)
            .map(|i| Event {
                sequence: u64::from(i) + 1,
                operation: [Operation::Replace, Operation::Snapshot, Operation::Read]
                    [i as usize % 3],
                error: None,
            })
            .collect()
    } else {
        Vec::new()
    };
    if detailed {
        diagnostics_after.dropped_events =
            Some(u64::from(input.measure) * 3 - u64::from(input.capacity));
    }
    Output {
        version: 1,
        input: input.clone(),
        path: recipe.path.clone(),
        build: recipe.baseline.build.clone(),
        setup_ns: 100,
        warmup: Phase {
            elapsed_ns: 100,
            completed: input.warmup,
        },
        measurement: Phase {
            elapsed_ns: 10_000_000,
            completed: input.measure,
        },
        error: None,
        read_values: observed,
        final_values: rows,
        old_values: input
            .pin_old
            .then(|| (0..input.row_count).map(i64::from).collect()),
        latency_ns: vec![1_000_000; 4096],
        metrics_before: metrics,
        metrics_after: after,
        diagnostics_before: diagnostics,
        diagnostics_after,
        events,
        events_available: true,
        cpu_ns: Some(10_000_000),
        process_peak_rss_bytes: Some(4 * 1024 * 1024),
    }
}
fn samples(recipe: &Recipe) -> Vec<RunSample> {
    let a = output(recipe, false);
    let b = output(recipe, true);
    recipe
        .pairs
        .iter()
        .flat_map(|pair| {
            [
                RunSample {
                    run: pair.baseline_run,
                    output: Some(a.clone()),
                    external_ns: Some(20_000_000),
                    error: None,
                },
                RunSample {
                    run: pair.candidate_run,
                    output: Some(b.clone()),
                    external_ns: Some(20_000_000),
                    error: None,
                },
            ]
        })
        .collect()
}
#[test]
fn fixed_effect_vectors_and_exact_boundaries() {
    let recipe = recipe();
    for (cost, expected) in [
        (10_000_000, "NO_MATERIAL_CHANGE"),
        (9_000_000, "IMPROVED"),
        (11_000_000, "REGRESSED"),
        (9_500_000, "NO_MATERIAL_CHANGE"),
        (10_500_000, "NO_MATERIAL_CHANGE"),
        (10_500_009, "REGRESSED"),
    ] {
        let mut samples = samples(&recipe);
        for sample in samples.iter_mut().skip(1).step_by(2) {
            sample.output.as_mut().unwrap().measurement.elapsed_ns = cost;
        }
        let result = analyze(&recipe, &samples);
        assert_eq!(result.validity, "VALID");
        assert_eq!(result.correctness, "PASS");
        assert_eq!(result.performance_effect, expected);
        assert_eq!(result.promotion, "INCOMPLETE");
    }
    let mut samples = samples(&recipe);
    // Both order strata contain low/high values; uncertainty cannot become improvement.
    for (i, sample) in samples.iter_mut().skip(1).step_by(2).enumerate() {
        sample.output.as_mut().unwrap().measurement.elapsed_ns =
            if i % 2 == 0 { 9_400_000 } else { 10_400_000 };
    }
    assert_eq!(
        analyze(&recipe, &samples).performance_effect,
        "INCONCLUSIVE"
    );
}
#[test]
fn order_drift_and_overflow_cannot_make_a_valid_comparison() {
    let recipe = recipe();
    let mut samples = samples(&recipe);
    for (i, pair) in recipe.pairs.iter().enumerate() {
        samples[i * 2 + 1]
            .output
            .as_mut()
            .unwrap()
            .measurement
            .elapsed_ns = if pair.order == test_controller::performance::Order::AB {
            8_000_000
        } else {
            10_000_000
        };
    }
    let result = analyze(&recipe, &samples);
    assert_eq!(result.validity, "INVALID");
    assert_eq!(result.performance_effect, "INCONCLUSIVE");
    for sample in &mut samples {
        sample.output.as_mut().unwrap().measurement.elapsed_ns = u64::MAX;
        sample.external_ns = None;
    }
    let result = analyze(&recipe, &samples);
    assert_eq!(result.validity, "INVALID");
    assert!(result.reasons.iter().any(|r| r.contains("overflow")));
}
#[test]
fn exact_guardrail_boundaries_unknown_observations_and_p99_rank() {
    let recipe = recipe();
    for (limit, field) in [
        (11_500_000, 0),
        (11_500_000, 1),
        (23_000_000, 2),
        (1_250_000, 3),
    ] {
        for offset in [0, 9] {
            let mut samples = samples(&recipe);
            let candidate = &mut samples[1];
            match field {
                0 => candidate.output.as_mut().unwrap().measurement.elapsed_ns = limit + offset,
                1 => candidate.output.as_mut().unwrap().cpu_ns = Some(limit + offset),
                2 => candidate.external_ns = Some(limit + offset),
                _ => {
                    let values = &mut candidate.output.as_mut().unwrap().latency_ns;
                    values[4055..].fill(limit + offset);
                }
            }
            assert_eq!(
                analyze(&recipe, &samples).guardrails,
                if offset == 0 { "PASS" } else { "FAIL" }
            );
        }
    }
    let mut samples = samples(&recipe);
    samples[1].output.as_mut().unwrap().cpu_ns = None;
    let result = analyze(&recipe, &samples);
    assert_eq!(result.guardrails, "UNKNOWN");
    assert_eq!(result.promotion, "INCOMPLETE");
    samples[1].output.as_mut().unwrap().cpu_ns = Some(0);
    samples[0].output.as_mut().unwrap().cpu_ns = Some(0);
    assert_eq!(analyze(&recipe, &samples).guardrails, "UNKNOWN");
    samples[1].output.as_mut().unwrap().process_peak_rss_bytes = Some(8 * 1024 * 1024 + 1);
    assert_eq!(analyze(&recipe, &samples).guardrails, "FAIL");
}
#[test]
fn independent_scalar_rejects_changed_seed_warmup_reads_final_and_old_rows() {
    let recipe = recipe();
    let good = output(&recipe, true);
    let mut input = recipe.fixture.clone();
    input.mode = DiagnosticsMode::Always;
    assert!(check_output(&input, &recipe.candidate, &good).is_ok());
    for case in 0..7 {
        let mut bad = good.clone();
        match case {
            0 => bad.input.seed += 1,
            1 => bad.input.warmup -= 1,
            2 => bad.read_values[32768] ^= 1,
            3 => bad.final_values[0] ^= 1,
            4 => bad.old_values.as_mut().unwrap()[0] ^= 1,
            5 => {
                bad.read_values.pop();
            }
            _ => bad.measurement.completed -= 1,
        }
        assert!(
            check_output(&input, &recipe.candidate, &bad).is_err(),
            "case {case}"
        );
    }
}
#[test]
fn all_mode_coverage_and_gauges_preserve_meaning() {
    let mut recipe = recipe();
    for rows in [1, 128] {
        for pinned in [false, true] {
            for capacity in [0, 1, 256] {
                for mode in [
                    DiagnosticsMode::Off,
                    DiagnosticsMode::Always,
                    DiagnosticsMode::Detailed,
                ] {
                    recipe.fixture.row_count = rows;
                    recipe.fixture.pin_old = pinned;
                    recipe.fixture.capacity = capacity;
                    recipe.candidate.mode = mode;
                    let observed = output(&recipe, true);
                    assert!(check_output(&observed.input, &recipe.candidate, &observed).is_ok());
                }
            }
        }
    }
}
#[test]
fn missing_failed_or_saturated_runs_never_disappear() {
    let recipe = recipe();
    for case in 0..8 {
        let mut samples = samples(&recipe);
        match case {
            0 => {
                samples.pop();
            }
            1 => samples[1].error = Some("timeout".into()),
            2 => samples[1].output = None,
            3 => samples[1].output.as_mut().unwrap().measurement.completed -= 1,
            4 => samples[1].output.as_mut().unwrap().path = "unreached".into(),
            5 => {
                samples[1]
                    .output
                    .as_mut()
                    .unwrap()
                    .diagnostics_after
                    .saturated = Some(true)
            }
            6 => samples[1].output.as_mut().unwrap().events_available = false,
            _ => samples[1].output.as_mut().unwrap().measurement.elapsed_ns = 0,
        }
        let result = analyze(&recipe, &samples);
        assert_eq!(result.validity, "INVALID", "case {case}");
        assert_eq!(result.performance_effect, "INCONCLUSIVE");
    }
}
#[test]
fn strict_recipe_limit_fields_order_identity_and_policy_are_frozen() {
    let recipe = recipe();
    let bytes = serde_json::to_vec(&recipe).unwrap();
    assert!(Recipe::parse(&bytes).is_ok());
    let text = String::from_utf8(bytes).unwrap();
    assert!(Recipe::parse(text.replacen("{", "{\"version\":1,", 1).as_bytes()).is_err());
    assert!(Recipe::parse(text.replacen("{", "{\"unknown\":1,", 1).as_bytes()).is_err());
    assert!(Recipe::parse(&vec![b' '; 65537]).is_err());
    for case in 0..6 {
        let mut changed = recipe.clone();
        match case {
            0 => changed.policy = "candidate-owned".into(),
            1 => changed.pairs[0].baseline_run = changed.pairs[0].candidate_run,
            2 => changed.order_seed += 1,
            3 => changed.candidate.config_sha256 = "e".repeat(64),
            4 => changed.candidate.build.profile = "debug".into(),
            _ => changed.candidate.build.source_sha256 = "e".repeat(64),
        }
        assert!(changed.validate().is_err(), "case {case}");
    }
}
#[test]
fn winner_reuse_reconfirmation_and_changed_instrumentation_are_rejected() {
    let mut recipe = recipe();
    recipe.kind = CampaignKind::Confirmation;
    recipe.candidate.mode = DiagnosticsMode::Off;
    recipe.provenance.confirmation_attempt = 1;
    recipe.provenance.exploration = vec![Exploration {
        candidate_sha256: recipe.candidate.worker_sha256.clone(),
        contract_sha256: recipe.comparison_contract_sha256().unwrap(),
        runs: vec![[42; 16]],
    }];
    assert!(recipe.validate().is_ok());
    for case in 0..8 {
        let mut changed = recipe.clone();
        match case {
            0 => changed.provenance.exploration[0]
                .runs
                .push(changed.pairs[0].candidate_run),
            1 => changed.provenance.confirmation_attempt = 2,
            2 => changed.provenance.previous_confirmation_runs.push([43; 16]),
            3 => changed.candidate.mode = DiagnosticsMode::Always,
            4 => changed.provenance.exploration[0].candidate_sha256 = "f".repeat(64),
            5 => {
                changed.provenance.exploration = vec![changed.provenance.exploration[0].clone(); 17]
            }
            6 => changed.baseline.worker_sha256 = "e".repeat(64),
            _ => changed.pairs.pop().map(|_| ()).unwrap(),
        }
        assert!(changed.validate().is_err(), "case {case}");
    }
}

fn write_json(path: &std::path::Path, value: &impl serde::Serialize) {
    std::fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}
// A literal external journal fixture. It never starts an engine or pretends to be
// product execution evidence; it tests the bundle verifier's trust boundaries.
fn saved_bundle() -> (tempfile::TempDir, String) {
    use test_controller::performance::{BuildReceipt, Order};
    use test_controller::{file_sha256, sha256};
    use test_protocol::{
        engine::{Action, Observation, Request, Response},
        frame,
    };
    let root = tempfile::tempdir().unwrap();
    let directory = root.path();
    let worker = b"synthetic non-executable worker bytes";
    let config = b"{\"synthetic\":\"fixed-vector\"}";
    let mut identity = side();
    identity.worker_sha256 = sha256(worker);
    identity.config_sha256 = sha256(config);
    let receipt = BuildReceipt {
        schema_version: 1,
        worker_sha256: identity.worker_sha256.clone(),
        build: identity.build.clone(),
        command: "synthetic bundle fixture, not a real build".into(),
        features: vec![],
        config_sha256: identity.config_sha256.clone(),
    };
    let receipt_bytes = serde_json::to_vec(&receipt).unwrap();
    identity.receipt_sha256 = sha256(&receipt_bytes);
    let recipe = Recipe::instrumentation([7; 16], 7, identity, DiagnosticsMode::Always).unwrap();
    let recipe_bytes = serde_json::to_vec(&recipe).unwrap();
    let recipe_hash = sha256(&recipe_bytes);
    std::fs::write(directory.join("recipe.json"), &recipe_bytes).unwrap();
    for name in ["baseline", "candidate"] {
        std::fs::write(directory.join(format!("{name}-worker")), worker).unwrap();
        std::fs::write(directory.join(format!("{name}-config.json")), config).unwrap();
        std::fs::write(
            directory.join(format!("{name}-receipt.json")),
            &receipt_bytes,
        )
        .unwrap();
    }
    std::fs::write(directory.join("environment.json"), b"{}").unwrap();
    std::fs::write(
        directory.join("controller"),
        b"synthetic recording controller",
    )
    .unwrap();
    std::fs::write(directory.join("resume.md"), b"Synthetic verifier fixture").unwrap();
    let mut top = std::collections::BTreeMap::new();
    for name in [
        "recipe.json",
        "baseline-worker",
        "candidate-worker",
        "baseline-receipt.json",
        "candidate-receipt.json",
        "baseline-config.json",
        "candidate-config.json",
        "controller",
        "environment.json",
        "resume.md",
    ] {
        top.insert(name, file_sha256(&directory.join(name)).unwrap());
    }
    let mut records = Vec::new();
    for (i, pair) in recipe.pairs.iter().enumerate() {
        for candidate in [false, true] {
            let observed = output(&recipe, candidate);
            let run = if candidate {
                pair.candidate_run
            } else {
                pair.baseline_run
            };
            let name = format!("run-{}", i * 2 + usize::from(candidate));
            let native = directory.join(&name);
            std::fs::create_dir(&native).unwrap();
            std::fs::write(native.join("worker"), worker).unwrap();
            let manifest = serde_json::json!({"schema_version":1,"journal_format_version":1,"run":run,
            "worker_sha256":recipe.baseline.worker_sha256,"source_sha256":recipe.baseline.build.source_sha256,
            "toolchain":recipe.baseline.build.toolchain,"target":recipe.baseline.build.target,"profile":"release","self_test":false});
            write_json(&native.join("manifest.json"), &manifest);
            let mut journal = Vec::new();
            let mut stdout = Vec::new();
            let mut sequence = 0u64;
            let mut record = |kind: u8, time: u64, id: u64, payload: &[u8]| {
                journal.push(kind);
                journal.extend_from_slice(&sequence.to_be_bytes());
                journal.extend_from_slice(&time.to_be_bytes());
                journal.extend_from_slice(&id.to_be_bytes());
                journal.extend_from_slice(&(payload.len() as u32).to_be_bytes());
                journal.extend_from_slice(payload);
                sequence += 1;
            };
            for (id, action, observation) in [
                (
                    1,
                    Action::Initialize {
                        database: [2; 16],
                        timeline: 7,
                        epoch: 3,
                        resident_bytes: 64 * 1024 * 1024,
                        snapshot_pins: 32,
                    },
                    Observation::Ready {},
                ),
                (
                    2,
                    Action::Performance {
                        input: observed.input.clone(),
                    },
                    Observation::Performance {
                        output: Box::new(observed),
                    },
                ),
            ] {
                let request = frame::encode(&Request {
                    version: 1,
                    run,
                    request: id,
                    actor: 0,
                    action,
                })
                .unwrap();
                let response = frame::encode(&Response {
                    version: 1,
                    run,
                    request: id,
                    actor: 0,
                    observation,
                })
                .unwrap();
                record(1, id * 20_000_000, id, &request);
                record(2, id * 20_000_000 + 1, id, &[]);
                record(3, id * 20_000_000 + 20_000_000, id, &response);
                stdout.extend_from_slice(&response);
            }
            record(5, 60_000_000, 0, &[]);
            std::fs::write(native.join("history.bin"), &journal).unwrap();
            std::fs::write(native.join("stdout.bin"), &stdout).unwrap();
            std::fs::write(native.join("stderr.bin"), b"").unwrap();
            let completion = serde_json::json!({"completion":"TRANSPORT_CLOSED_NO_PRODUCT_VERDICT","worker_sha256":recipe.baseline.worker_sha256,
            "requests":2,"observations":2,"history_sha256":sha256(&journal),"stdout_sha256":sha256(&stdout),"stderr_sha256":sha256(b"")});
            write_json(&native.join("completion.json"), &completion);
            let mut files = std::collections::BTreeMap::new();
            for name in [
                "worker",
                "manifest.json",
                "history.bin",
                "stdout.bin",
                "stderr.bin",
                "completion.json",
            ] {
                files.insert(name, file_sha256(&native.join(name)).unwrap());
            }
            records.push(serde_json::json!({"ordinal":i*2+usize::from(candidate != (pair.order==Order::BA)),"run":run,"candidate":candidate,"directory":name,"failure":null,"files":files}));
        }
    }
    let index = serde_json::json!({"schema_version":1,"recipe_sha256":recipe_hash,"controller":recipe.baseline.build,
        "files":top,"runs":records,"original_recipe_sha256":null});
    write_json(&directory.join("index.json"), &index);
    (root, recipe_hash)
}
#[test]
fn bundle_verifier_reanalyzes_raw_streams_and_rejects_missing_history_identity_and_failure() {
    use test_controller::performance::verify;
    let (root, hash) = saved_bundle();
    let directory = root.path();
    let good = verify(directory, &hash).unwrap();
    assert_eq!(good.exit_code(), 0);
    assert_eq!(good.comparison.promotion, "INCOMPLETE");
    assert_eq!(good.original_controller.toolchain, "fixed vector");
    assert_ne!(
        good.analyzer_build.toolchain,
        good.original_controller.toolchain
    );
    assert_eq!(
        good.analyzer_sha256,
        test_controller::file_sha256(&std::env::current_exe().unwrap()).unwrap()
    );
    for name in [
        "baseline-config.json",
        "candidate-config.json",
        "controller",
    ] {
        let file = directory.join(name);
        let original = std::fs::read(&file).unwrap();
        std::fs::remove_file(&file).unwrap();
        assert!(verify(directory, &hash).is_err());
        std::fs::write(&file, b"changed identity").unwrap();
        assert!(verify(directory, &hash).is_err());
        std::fs::write(&file, original).unwrap();
    }
    // A saved verdict has no authority: reanalysis always uses the raw observations.
    write_json(
        &directory.join("comparison.json"),
        &serde_json::json!({"promotion":"READY_FOR_REVIEW","correctness":"PASS"}),
    );
    assert_eq!(
        verify(directory, &hash).unwrap().comparison.promotion,
        "INCOMPLETE"
    );
    let journal = directory.join("run-1/history.bin");
    let original = std::fs::read(&journal).unwrap();
    std::fs::remove_file(&journal).unwrap();
    assert_eq!(
        verify(directory, &hash).unwrap().comparison.validity,
        "INVALID"
    );
    std::fs::write(&journal, &original[..original.len() - 1]).unwrap();
    assert_eq!(
        verify(directory, &hash).unwrap().comparison.validity,
        "INVALID"
    );
    std::fs::write(&journal, &original).unwrap();
    std::fs::write(
        directory.join("run-1/failure.json"),
        b"{\"kind\":\"Timeout\"}",
    )
    .unwrap();
    assert_eq!(
        verify(directory, &hash).unwrap().comparison.validity,
        "INVALID"
    );
    std::fs::remove_file(directory.join("run-1/failure.json")).unwrap();
    std::fs::write(directory.join("candidate-worker"), b"wrong worker").unwrap();
    assert!(verify(directory, &hash).is_err());
}
#[test]
fn replay_audit_preserves_original_ids_and_cannot_promote() {
    let original = recipe();
    let mut replay = Recipe::instrumentation([8; 16], 7, side(), DiagnosticsMode::Always).unwrap();
    replay.kind = CampaignKind::ReplayAudit;
    replay.provenance.original_recipe_sha256 = Some("f".repeat(64));
    replay.provenance.previous_confirmation_runs = original
        .pairs
        .iter()
        .flat_map(|p| [p.baseline_run, p.candidate_run])
        .collect();
    assert!(replay.validate().is_ok());
    let result = analyze(&replay, &samples(&replay));
    assert_eq!(result.promotion, "INCOMPLETE");
    replay.pairs[0].candidate_run = original.pairs[0].candidate_run;
    assert!(replay.validate().is_err());
}

#[test]
fn impossible_timing_is_invalid_but_equal_duration_and_missing_external_are_distinct() {
    let recipe = recipe();
    let mut valid = samples(&recipe);
    let measured = valid[1].output.as_ref().unwrap();
    valid[1].external_ns =
        Some(measured.setup_ns + measured.warmup.elapsed_ns + measured.measurement.elapsed_ns);
    assert_eq!(analyze(&recipe, &valid).validity, "VALID");
    valid[1].output.as_mut().unwrap().latency_ns[0] = 10_000_000;
    assert_eq!(analyze(&recipe, &valid).validity, "VALID");
    for case in 0..3 {
        let mut invalid = valid.clone();
        match case {
            0 => invalid[1].external_ns = invalid[1].external_ns.map(|ns| ns - 1),
            1 => invalid[1].output.as_mut().unwrap().latency_ns[0] += 1,
            _ => {
                let measured = invalid[1].output.as_mut().unwrap();
                measured.setup_ns = u64::MAX;
                measured.warmup.elapsed_ns = u64::MAX;
                measured.measurement.elapsed_ns = u64::MAX;
                invalid[1].external_ns = Some(u64::MAX);
            }
        }
        let report = analyze(&recipe, &invalid);
        assert_eq!(report.validity, "INVALID");
        assert_eq!(report.performance_effect, "INCONCLUSIVE");
        assert!(
            report
                .reasons
                .iter()
                .any(|reason| reason == "impossible nested timing durations")
        );
    }
    valid[1].external_ns = None;
    let report = analyze(&recipe, &valid);
    assert_eq!(report.validity, "VALID");
    assert_eq!(report.guardrails, "UNKNOWN");
    assert_eq!(report.promotion, "INCOMPLETE");
}

#[test]
fn detailed_events_reject_wrong_labels_stale_warmup_sequences_and_gaps() {
    let mut recipe = recipe();
    recipe.candidate.mode = DiagnosticsMode::Detailed;
    let good = output(&recipe, true);
    assert!(check_output(&good.input, &recipe.candidate, &good).is_ok());
    for case in 0..4 {
        let mut bad = good.clone();
        match case {
            0 => bad.events[1].operation = Operation::Replace,
            1 => bad.events[0].sequence = 0,
            2 => bad.events[1].sequence += 1,
            _ => {
                bad.events.pop();
                bad.diagnostics_after.dropped_events =
                    bad.diagnostics_after.dropped_events.map(|count| count + 1);
            }
        }
        assert!(check_output(&bad.input, &recipe.candidate, &bad).is_err());
    }
    // Before counters include warmup and setup completions. A stale pre-drain
    // event remains increasing but is invalid relative to this measurement cut.
    let mut measured = good.clone();
    measured.diagnostics_before.operations.as_mut().unwrap()[3].completed = 100;
    measured.diagnostics_after.operations.as_mut().unwrap()[3].completed += 100;
    for event in &mut measured.events {
        event.sequence += 100;
    }
    assert!(check_output(&measured.input, &recipe.candidate, &measured).is_ok());
    measured.events[0].sequence -= 1;
    assert!(check_output(&measured.input, &recipe.candidate, &measured).is_err());
}
