use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
use test_controller::{RunOptions, RunReport, file_sha256, run, sha256, spec, verdict::*};

const CASE: &str = include_str!("../../../../tests/cases/dev0-smoke.json");
const WORKER: &str = env!("CARGO_BIN_EXE_xadbtest-mutant-worker");
const CLI: &str = env!("CARGO_BIN_EXE_xadbtest");

fn config(root: &Path, mode: &str) -> RunOptions {
    let case = root.join("case.json");
    fs::write(&case, CASE).unwrap();
    RunOptions {
        case,
        expected_case_sha256: sha256(CASE.as_bytes()),
        worker: WORKER.into(),
        expected_worker_sha256: file_sha256(Path::new(WORKER)).unwrap(),
        artifacts: std::env::var_os("XADBTEST_SELF_TEST_ARTIFACTS")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| root.join("evidence")),
        worker_args: vec![mode.into()],
        inject_cleanup_failure: false,
    }
}

fn verify_bundle(report: &RunReport) -> Value {
    let read = |name: &str| fs::read(report.directory.join(name)).unwrap();
    let stored: Verdict = serde_json::from_slice(&read("verdict.json")).unwrap();
    assert_eq!(stored.case_verdict, report.verdict.case_verdict);
    assert_eq!(stored.exit_code(), report.verdict.exit_code());
    let artifacts: std::collections::BTreeMap<String, String> =
        serde_json::from_slice(&read("artifacts.json")).unwrap();
    for (name, expected) in artifacts {
        assert_eq!(sha256(&read(&name)), expected);
    }
    let manifest: Value = serde_json::from_slice(&read("manifest.json")).unwrap();
    assert!(!Path::new(manifest["work_directory"].as_str().unwrap()).exists());
    for (sequence, line) in String::from_utf8(read("receipts.jsonl"))
        .unwrap()
        .lines()
        .enumerate()
    {
        let receipt: Value = serde_json::from_str(line).unwrap();
        assert_eq!(receipt["sequence"], sequence);
        if receipt["event"] == "worker.reaped" {
            let pid = receipt["detail"]["pid"].as_u64().unwrap();
            assert!(
                !Path::new(&format!("/proc/{pid}")).exists(),
                "owned leader must be reaped"
            );
        }
    }
    manifest
}

#[test]
fn independent_vectors_and_complete_receipts() {
    let root = tempfile::tempdir().unwrap();
    let report = run(&config(root.path(), "correct")).unwrap();
    assert_eq!(report.verdict.case_verdict, CaseVerdict::Pass);
    assert_eq!(report.verdict.assertions_checked, 7);
    let manifest = verify_bundle(&report);
    assert_eq!(manifest["case_sha256"], sha256(CASE.as_bytes()));
    let request: Value =
        serde_json::from_slice(&fs::read(report.directory.join("request.json")).unwrap()).unwrap();
    assert!(request.get("oracle").is_none());
    assert!(!request.to_string().contains("expected"));
}

#[test]
fn incorrect_results_cannot_pass() {
    let root = tempfile::tempdir().unwrap();
    let report = run(&config(root.path(), "wrong-value")).unwrap();
    assert_eq!(report.verdict.product_verdict, ProductVerdict::Fail);
    assert_eq!(report.verdict.case_verdict, CaseVerdict::Fail);
    assert_eq!(report.verdict.exit_code(), 1);
    verify_bundle(&report);
}

#[test]
fn worker_cannot_supply_verdict_or_invalid_observations() {
    for mode in [
        "plain-pass",
        "forged-pass",
        "malformed",
        "wrong-run",
        "wrong-case",
        "wrong-version",
        "wrong-sequence",
        "missing-step",
        "duplicate",
        "extra",
        "truncated",
        "missing-witness",
        "unknown-witness",
    ] {
        let root = tempfile::tempdir().unwrap();
        let report = run(&config(root.path(), mode)).unwrap();
        assert_eq!(report.verdict.case_verdict, CaseVerdict::Invalid, "{mode}");
        assert_ne!(
            report.verdict.product_verdict,
            ProductVerdict::Pass,
            "{mode}"
        );
        verify_bundle(&report);
    }
}

#[test]
fn wrong_binary_or_case_is_rejected_before_spawn() {
    for wrong in ["worker", "case", "placeholder", "modified-oracle"] {
        let root = tempfile::tempdir().unwrap();
        let mut options = config(root.path(), "correct");
        match wrong {
            "worker" => options.expected_worker_sha256 = "0".repeat(64),
            "case" => options.expected_case_sha256 = "0".repeat(64),
            "placeholder" => options.expected_worker_sha256 = "sha256".into(),
            _ => {
                let mut case: Value = serde_json::from_str(CASE).unwrap();
                case["oracle"]["assertions"][0]["expected"]["value"] = json!(123);
                fs::write(&options.case, serde_json::to_vec(&case).unwrap()).unwrap();
            }
        }
        let report = run(&options).unwrap();
        assert_eq!(report.verdict.case_verdict, CaseVerdict::Invalid);
        assert_eq!(report.verdict.product_verdict, ProductVerdict::NotEvaluated);
        assert!(
            !fs::read_to_string(report.directory.join("receipts.jsonl"))
                .unwrap()
                .contains("worker.spawned")
        );
        verify_bundle(&report);
    }
}

#[test]
fn schema_rejects_missing_oracle_zero_assertions_and_unsupported_inputs() {
    let original: Value = serde_json::from_str(CASE).unwrap();
    let mut cases = vec![];
    let mut case = original.clone();
    case.as_object_mut().unwrap().remove("oracle");
    cases.push(case);
    let mut case = original.clone();
    case["oracle"]["assertions"] = json!([]);
    cases.push(case);
    let mut case = original.clone();
    case["oracle"]["id"] = json!("candidate-self-report");
    cases.push(case);
    let mut case = original.clone();
    case["steps"][0]["action"]["operation"] = json!("shell");
    cases.push(case);
    let mut case = original.clone();
    case["scenario_schema_version"] = json!(2);
    cases.push(case);
    let mut case = original.clone();
    case["extra_field"] = json!(true);
    cases.push(case);
    let mut case = original.clone();
    case["steps"][0]["id"] = json!("negative");
    cases.push(case);
    let mut case = original.clone();
    case["oracle"]["assertions"][0]["step_id"] = json!("unknown");
    cases.push(case);
    let mut case = original.clone();
    case["oracle"]["assertions"][0]["expected"]["type"] = json!("float");
    cases.push(case);
    let mut case = original.clone();
    case["required_witnesses"] = json!([]);
    cases.push(case);
    let mut case = original.clone();
    case["limits"]["timeout_ms"] = json!(0);
    cases.push(case);
    let mut case = original.clone();
    case["limits"]["output_bytes_per_stream"] = json!(0);
    cases.push(case);
    let mut case = original.clone();
    case["steps"] = json!([]);
    cases.push(case);
    for case in cases {
        let root = tempfile::tempdir().unwrap();
        let mut options = config(root.path(), "correct");
        let bytes = serde_json::to_vec(&case).unwrap();
        options.expected_case_sha256 = sha256(&bytes);
        fs::write(&options.case, bytes).unwrap();
        let report = run(&options).unwrap();
        assert_eq!(report.verdict.case_verdict, CaseVerdict::Invalid, "{case}");
        assert_eq!(report.verdict.assertions_checked, 0);
        verify_bundle(&report);
    }
    assert!(spec::parse(&vec![b' '; 65_537]).is_err());
    assert!(spec::parse(br#"{"scenario_schema_version":1,"scenario_schema_version":1}"#).is_err());
}

#[test]
fn timeout_kills_and_reaps_owned_worker() {
    let root = tempfile::tempdir().unwrap();
    let mut options = config(root.path(), "timeout");
    let mut case: Value = serde_json::from_str(CASE).unwrap();
    case["limits"]["timeout_ms"] = json!(100);
    let bytes = serde_json::to_vec(&case).unwrap();
    options.expected_case_sha256 = sha256(&bytes);
    fs::write(&options.case, bytes).unwrap();
    let report = run(&options).unwrap();
    assert_eq!(report.verdict.case_verdict, CaseVerdict::Timeout);
    assert_eq!(report.verdict.execution_state, ExecutionState::Timeout);
    assert_eq!(report.verdict.cleanup_state, CleanupState::Pass);
    verify_bundle(&report);
}

#[test]
fn bounded_stdout_and_stderr_reject_flooding() {
    for mode in ["flood", "stderr-flood"] {
        let root = tempfile::tempdir().unwrap();
        let report = run(&config(root.path(), mode)).unwrap();
        assert_eq!(report.verdict.execution_state, ExecutionState::Error);
        assert_ne!(report.verdict.case_verdict, CaseVerdict::Pass);
        for file in ["observations.jsonl", "stderr.txt"] {
            assert!(fs::metadata(report.directory.join(file)).unwrap().len() <= 65_536);
        }
        verify_bundle(&report);
    }
}

#[test]
fn nonzero_exit_cannot_pass_despite_correct_values() {
    let root = tempfile::tempdir().unwrap();
    let report = run(&config(root.path(), "nonzero")).unwrap();
    assert_eq!(report.verdict.product_verdict, ProductVerdict::Pass);
    assert_eq!(report.verdict.execution_state, ExecutionState::Error);
    assert_eq!(report.verdict.case_verdict, CaseVerdict::Error);
    verify_bundle(&report);
}

#[test]
fn cleanup_failure_cannot_erase_failure_or_promote_success() {
    for mode in ["correct", "wrong-value"] {
        let root = tempfile::tempdir().unwrap();
        let mut options = config(root.path(), mode);
        options.inject_cleanup_failure = true;
        let report = run(&options).unwrap();
        assert_eq!(report.verdict.cleanup_state, CleanupState::Fail);
        if mode == "wrong-value" {
            assert_eq!(report.verdict.product_verdict, ProductVerdict::Fail);
            assert_eq!(report.verdict.case_verdict, CaseVerdict::Fail);
        } else {
            assert_eq!(report.verdict.product_verdict, ProductVerdict::Pass);
            assert_eq!(report.verdict.case_verdict, CaseVerdict::Error);
        }
        assert_eq!(verify_bundle(&report)["cleanup_failure_injected"], true);
    }
}

#[test]
fn failure_observed_before_timeout_is_preserved() {
    let root = tempfile::tempdir().unwrap();
    let report = run(&config(root.path(), "wrong-value-then-hang")).unwrap();
    assert_eq!(report.verdict.execution_state, ExecutionState::Timeout);
    assert_eq!(report.verdict.product_verdict, ProductVerdict::Fail);
    assert_eq!(report.verdict.case_verdict, CaseVerdict::Fail);
    verify_bundle(&report);
}

#[test]
fn retry_and_concurrent_runs_do_not_overwrite_evidence() {
    let root = tempfile::tempdir().unwrap();
    let options = config(root.path(), "correct");
    let reports: Vec<_> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..4)
            .map(|_| scope.spawn(|| run(&options).unwrap()))
            .collect();
        workers.into_iter().map(|w| w.join().unwrap()).collect()
    });
    let mut directories = std::collections::BTreeSet::new();
    for report in &reports {
        assert!(directories.insert(&report.directory));
        assert_eq!(report.verdict.case_verdict, CaseVerdict::Pass);
        verify_bundle(report);
    }
    let mut bad = options;
    bad.worker_args = vec!["wrong-value".into()];
    let failed = run(&bad).unwrap();
    let saved = fs::read(failed.directory.join("verdict.json")).unwrap();
    bad.worker_args = vec!["correct".into()];
    assert_eq!(run(&bad).unwrap().verdict.case_verdict, CaseVerdict::Pass);
    assert_eq!(
        saved,
        fs::read(failed.directory.join("verdict.json")).unwrap()
    );
}

#[test]
fn executable_spawn_failure_retains_evidence() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let mut options = config(root.path(), "correct");
    options.worker = root.path().join("not-executable");
    fs::write(&options.worker, b"not executable").unwrap();
    fs::set_permissions(&options.worker, fs::Permissions::from_mode(0o600)).unwrap();
    options.expected_worker_sha256 = file_sha256(&options.worker).unwrap();
    let report = run(&options).unwrap();
    assert_eq!(report.verdict.case_verdict, CaseVerdict::Error);
    assert_eq!(report.verdict.cleanup_state, CleanupState::Pass);
    verify_bundle(&report);
}

#[test]
fn unavailable_input_is_an_execution_error_not_an_invalid_case() {
    let root = tempfile::tempdir().unwrap();
    let mut options = config(root.path(), "correct");
    options.worker = root.path().join("missing-worker");
    let report = run(&options).unwrap();
    assert_eq!(report.verdict.case_verdict, CaseVerdict::Error);
    assert_eq!(report.verdict.execution_state, ExecutionState::Error);
    assert_ne!(report.verdict.case_validity, CaseValidity::Invalid);
    verify_bundle(&report);
}

#[test]
fn cli_exit_code_matches_persisted_verdict_and_rejects_unknown_arguments() {
    let root = tempfile::tempdir().unwrap();
    let options = config(root.path(), "correct");
    let invoke = |hash: &str| {
        Command::new(CLI)
            .args(["run", "--case"])
            .arg(&options.case)
            .args(["--case-sha256", &options.expected_case_sha256, "--worker"])
            .arg(&options.worker)
            .args(["--worker-sha256", hash, "--artifacts"])
            .arg(&options.artifacts)
            .output()
            .unwrap()
    };
    for (hash, expected) in [
        (options.expected_worker_sha256.as_str(), 0),
        ("0".repeat(64).as_str(), 2),
    ] {
        let output = invoke(hash);
        assert_eq!(output.status.code(), Some(expected));
        let receipt: Value = serde_json::from_slice(&output.stdout).unwrap();
        let verdict: Verdict = serde_json::from_slice(
            &fs::read(Path::new(receipt["run_directory"].as_str().unwrap()).join("verdict.json"))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(i32::from(verdict.exit_code()), expected);
    }
    assert_eq!(
        Command::new(CLI)
            .args(["run", "--shell", "echo PASS"])
            .output()
            .unwrap()
            .status
            .code(),
        Some(3)
    );
}
