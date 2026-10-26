//! Profile B slot gate vectors. The native mutant answers every request with Ready,
//! so campaigns here are INVALID comparisons; only gate and bundle boundaries are tested.
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    thread,
    time::Duration,
};
use test_controller::performance::{
    self, BuildReceipt, Options, Recipe, ReplayOptions, Side, gate,
};
use test_controller::{file_sha256, sha256};
use test_protocol::performance::{BuildIdentity, DiagnosticsMode};

const VERDICT: &str = "5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e";

struct Campaign {
    root: tempfile::TempDir,
    recipe: Recipe,
    recipe_sha256: String,
    profile_sha256: String,
}
impl Campaign {
    fn new(go_ms: u64, ack_ms: u64) -> Self {
        let root = tempfile::tempdir().unwrap();
        let path = root.path();
        let worker = path.join("worker");
        fs::copy(env!("CARGO_BIN_EXE_xadbtest-native-mutant-worker"), &worker).unwrap();
        let config = br#"{"synthetic":"gate vector"}"#;
        fs::write(path.join("config.json"), config).unwrap();
        let mut side = Side {
            worker_sha256: file_sha256(&worker).unwrap(),
            receipt_sha256: String::new(),
            build: BuildIdentity {
                source_sha256: "d".repeat(64),
                toolchain: "fixed vector".into(),
                target: "x86_64-unknown-linux-gnu".into(),
                profile: "release".into(),
                self_test: false,
            },
            config_sha256: sha256(config),
            mode: DiagnosticsMode::Off,
        };
        let receipt = serde_json::to_vec(&BuildReceipt {
            schema_version: 1,
            worker_sha256: side.worker_sha256.clone(),
            build: side.build.clone(),
            command: "synthetic gate fixture, not a real build".into(),
            features: vec![],
            config_sha256: side.config_sha256.clone(),
        })
        .unwrap();
        fs::write(path.join("receipt.json"), &receipt).unwrap();
        side.receipt_sha256 = sha256(&receipt);
        let recipe = Recipe::instrumentation([5; 16], 9, side, DiagnosticsMode::Always).unwrap();
        let bytes = serde_json::to_vec(&recipe).unwrap();
        fs::write(path.join("recipe.json"), &bytes).unwrap();
        let profile = serde_json::to_vec(&serde_json::json!({"schema_version":1,
            "protocol":gate::PROTOCOL,"go_deadline_ms":go_ms,"ack_deadline_ms":ack_ms,
            "environment":{"vector":"self-test only"}}))
        .unwrap();
        fs::write(path.join("profile.json"), &profile).unwrap();
        fs::create_dir(path.join("gate")).unwrap();
        for name in [gate::REQUEST_FIFO, gate::REPLY_FIFO] {
            nix::unistd::mkfifo(
                &path.join("gate").join(name),
                nix::sys::stat::Mode::from_bits_truncate(0o600),
            )
            .unwrap();
        }
        Self {
            recipe_sha256: sha256(&bytes),
            profile_sha256: sha256(&profile),
            recipe,
            root,
        }
    }
    fn path(&self) -> &Path {
        self.root.path()
    }
    fn gate(&self) -> gate::Options {
        gate::Options {
            profile: self.path().join("profile.json"),
            profile_sha256: self.profile_sha256.clone(),
            directory: self.path().join("gate"),
        }
    }
    fn options(&self, artifacts: &str, gated: bool) -> Options {
        let path = self.path();
        Options {
            recipe: path.join("recipe.json"),
            recipe_sha256: self.recipe_sha256.clone(),
            baseline_worker: path.join("worker"),
            candidate_worker: path.join("worker"),
            baseline_receipt: path.join("receipt.json"),
            candidate_receipt: path.join("receipt.json"),
            baseline_config: path.join("config.json"),
            candidate_config: path.join("config.json"),
            artifacts: path.join(artifacts),
            gate: gated.then(|| self.gate()),
        }
    }
    /// Execution-order run IDs, matching the controller's AB/BA slot order.
    fn ordered(&self) -> Vec<String> {
        self.recipe
            .pairs
            .iter()
            .flat_map(|p| match p.order {
                performance::Order::AB => [p.baseline_run, p.candidate_run],
                performance::Order::BA => [p.candidate_run, p.baseline_run],
            })
            .map(hex)
            .collect()
    }
}
fn hex(run: [u8; 16]) -> String {
    run.iter().map(|b| format!("{b:02x}")).collect()
}

enum Action {
    Reply(String),
    Delayed(Duration, String),
    Silent,
    Close,
}
/// Fake supervisor: holds both FIFO ends open (RDWR) so its own exit is the only EOF.
fn supervisor(
    directory: &Path,
    respond: impl Fn(&str) -> Action + Send + 'static,
) -> thread::JoinHandle<Vec<String>> {
    let rdwr = |name| {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(directory.join(name))
            .unwrap()
    };
    let request = rdwr(gate::REQUEST_FIFO);
    let mut reply = rdwr(gate::REPLY_FIFO);
    thread::spawn(move || {
        let mut seen = Vec::new();
        let mut lines = BufReader::new(request);
        loop {
            let mut line = String::new();
            lines.read_line(&mut line).unwrap();
            let line = line.trim_end().to_string();
            if line == "STOP" {
                return seen;
            }
            seen.push(line.clone());
            match respond(&line) {
                Action::Reply(text) => reply.write_all(format!("{text}\n").as_bytes()).unwrap(),
                Action::Delayed(delay, text) => {
                    thread::sleep(delay);
                    reply.write_all(format!("{text}\n").as_bytes()).unwrap();
                }
                Action::Silent => {}
                Action::Close => return seen,
            }
        }
    })
}
fn stop(campaign: &Campaign, handle: thread::JoinHandle<Vec<String>>) -> Vec<String> {
    if !handle.is_finished() {
        let mut fifo = OpenOptions::new()
            .write(true)
            .open(campaign.path().join("gate").join(gate::REQUEST_FIFO))
            .unwrap();
        fifo.write_all(b"STOP\n").unwrap();
    }
    handle.join().unwrap()
}
/// Default exchange: admit every slot and ACK, with the verdict on ordinal 19.
fn admit(line: &str) -> Action {
    let words: Vec<_> = line.split(' ').collect();
    match words[0] {
        "READY" => Action::Reply(format!("GO {} {}", words[1], words[2])),
        "DONE" if words[1] == "19" => {
            Action::Reply(format!("ACK {} {} {VERDICT}", words[1], words[2]))
        }
        "DONE" => Action::Reply(format!("ACK {} {}", words[1], words[2])),
        _ => panic!("unexpected controller line {line}"),
    }
}
fn bundle(artifacts: &Path) -> PathBuf {
    let mut found: Vec<_> = fs::read_dir(artifacts)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("performance-")
        })
        .collect();
    assert_eq!(found.len(), 1);
    found.pop().unwrap()
}
fn json(path: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn write_json(path: &Path, value: &serde_json::Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
/// Rebind one retained file in index.json so only the targeted check can reject it.
fn rebind(directory: &Path, name: &str) {
    let mut index = json(&directory.join("index.json"));
    index["files"][name] = file_sha256(&directory.join(name)).unwrap().into();
    write_json(&directory.join("index.json"), &index);
}

#[test]
fn gated_campaign_binds_profile_transcript_and_verdict_before_analysis() {
    let campaign = Campaign::new(2000, 2000);
    let handle = supervisor(&campaign.path().join("gate"), admit);
    let report = performance::run(&campaign.options("gated", true)).unwrap();
    let seen = stop(&campaign, handle);
    assert_eq!(seen.len(), 40);
    // The mutant never returns a measurement; the comparison stays invalid, never hidden.
    assert_eq!(report.comparison.validity, "INVALID");
    let directory = bundle(&campaign.path().join("gated"));
    let index = json(&directory.join("index.json"));
    assert_eq!(index["schema_version"], 2);
    assert_eq!(index["gate"]["profile_sha256"], campaign.profile_sha256);
    assert_eq!(index["gate"]["environment_sha256"], VERDICT);
    assert_eq!(index["files"].as_object().unwrap().len(), 12);
    let transcript = json(&directory.join("gate-transcript.json"));
    let lines = transcript["lines"].as_array().unwrap();
    assert_eq!(lines.len(), 80);
    let order = campaign.ordered();
    assert_eq!(lines[0]["text"], format!("READY 0 {}", order[0]));
    assert_eq!(lines[1]["from"], "supervisor");
    assert_eq!(lines[79]["text"], format!("ACK 19 {} {VERDICT}", order[19]));
    assert!(
        fs::read_to_string(directory.join("resume.md"))
            .unwrap()
            .contains("--gate-profile")
    );
    performance::verify(&directory, &campaign.recipe_sha256).unwrap();

    // Without the option, the original schema and inventory are unchanged.
    performance::run(&campaign.options("ungated", false)).unwrap();
    let plain = bundle(&campaign.path().join("ungated"));
    let index = json(&plain.join("index.json"));
    assert_eq!(index["schema_version"], 1);
    assert!(index.get("gate").is_none());
    assert_eq!(index["files"].as_object().unwrap().len(), 10);
    assert!(!plain.join("gate-transcript.json").exists());
    assert!(
        !fs::read_to_string(plain.join("resume.md"))
            .unwrap()
            .contains("slot gate")
    );

    // A changed transcript is rejected by identity, and when rebound, by sequence.
    let path = directory.join("gate-transcript.json");
    let original = fs::read(&path).unwrap();
    let mut duplicated = transcript.clone();
    let first = duplicated["lines"][0].clone();
    duplicated["lines"].as_array_mut().unwrap().insert(1, first);
    write_json(&path, &duplicated);
    assert!(performance::verify(&directory, &campaign.recipe_sha256).is_err());
    rebind(&directory, "gate-transcript.json");
    let error = performance::verify(&directory, &campaign.recipe_sha256).unwrap_err();
    assert!(error.to_string().contains("sequence"), "{error}");
    fs::write(&path, &original).unwrap();
    rebind(&directory, "gate-transcript.json");
    performance::verify(&directory, &campaign.recipe_sha256).unwrap();

    // The index digest must equal the final ACK; a different verdict cannot be substituted.
    let mut index = json(&directory.join("index.json"));
    let bound = index.clone();
    index["gate"]["environment_sha256"] = "6".repeat(64).into();
    write_json(&directory.join("index.json"), &index);
    assert!(performance::verify(&directory, &campaign.recipe_sha256).is_err());
    index["gate"]["environment_sha256"] = serde_json::Value::Null;
    write_json(&directory.join("index.json"), &index);
    let error = performance::verify(&directory, &campaign.recipe_sha256).unwrap_err();
    assert!(error.to_string().contains("analysis withheld"), "{error}");
    // A gated index cannot claim the ungated schema.
    index = bound.clone();
    index["schema_version"] = 1.into();
    write_json(&directory.join("index.json"), &index);
    assert!(performance::verify(&directory, &campaign.recipe_sha256).is_err());
    write_json(&directory.join("index.json"), &bound);

    // The retained profile is bound by both the file inventory and the gate binding.
    let profile = directory.join("gate-profile.json");
    let original = fs::read(&profile).unwrap();
    fs::write(&profile, b"{\"schema_version\":1}").unwrap();
    rebind(&directory, "gate-profile.json");
    assert!(performance::verify(&directory, &campaign.recipe_sha256).is_err());
    fs::write(&profile, original).unwrap();
    rebind(&directory, "gate-profile.json");
    performance::verify(&directory, &campaign.recipe_sha256).unwrap();
}

#[test]
fn abort_ends_campaign_with_verdict_and_not_run_slots() {
    let campaign = Campaign::new(2000, 2000);
    let handle = supervisor(&campaign.path().join("gate"), |line| {
        let words: Vec<_> = line.split(' ').collect();
        match (words[0], words[1]) {
            ("READY", "3") => Action::Reply(format!("ABORT 3 {}", words[2])),
            ("DONE", "3") => Action::Reply(format!("ACK 3 {} {VERDICT}", words[2])),
            _ => admit(line),
        }
    });
    let report = performance::run(&campaign.options("artifacts", true)).unwrap();
    let seen = stop(&campaign, handle);
    assert_eq!(seen.len(), 8);
    assert_eq!(report.comparison.validity, "INVALID");
    let directory = bundle(&campaign.path().join("artifacts"));
    let index = json(&directory.join("index.json"));
    for run in index["runs"].as_array().unwrap() {
        let ordinal = run["ordinal"].as_u64().unwrap();
        match ordinal {
            0..=2 => assert!(run["directory"].is_string() && run["failure"].is_null()),
            3 => assert_eq!(run["failure"], gate::ABORTED),
            _ => assert_eq!(run["failure"], gate::NOT_RUN),
        }
        if ordinal >= 3 {
            assert!(run["directory"].is_null());
        }
    }
    assert_eq!(
        fs::read_dir(&directory)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("pair-")
            })
            .count(),
        3
    );
    performance::verify(&directory, &campaign.recipe_sha256).unwrap();
    // An executed slot cannot be relabelled as aborted after the fact.
    let mut changed = index.clone();
    for run in changed["runs"].as_array_mut().unwrap() {
        if run["ordinal"] == 2 {
            run["failure"] = gate::ABORTED.into();
        }
    }
    write_json(&directory.join("index.json"), &changed);
    assert!(performance::verify(&directory, &campaign.recipe_sha256).is_err());
}

#[test]
fn gate_eof_deadline_mismatch_and_unsolicited_reply_withhold_analysis() {
    type Respond = fn(&str) -> Action;
    let vectors: [(&str, Respond, &str); 6] = [
        (
            "eof",
            |line| {
                if line.starts_with("READY 2 ") {
                    Action::Close
                } else {
                    admit(line)
                }
            },
            "EOF",
        ),
        (
            "deadline",
            |line| {
                if line.starts_with("READY 1 ") {
                    Action::Silent
                } else {
                    admit(line)
                }
            },
            "deadline",
        ),
        (
            "mismatch",
            |line| {
                if line.starts_with("READY 1 ") {
                    Action::Reply(format!("GO 1 {}", "0".repeat(32)))
                } else {
                    admit(line)
                }
            },
            "ordinal/run",
        ),
        (
            "unsolicited",
            |line| {
                let words: Vec<_> = line.split(' ').collect();
                if words[0] == "READY" && words[1] == "0" {
                    Action::Reply(format!("GO 0 {0}\nGO 0 {0}", words[2]))
                } else {
                    admit(line)
                }
            },
            "unsolicited",
        ),
        (
            "ack-without-verdict",
            |line| {
                let words: Vec<_> = line.split(' ').collect();
                if words[0] == "DONE" && words[1] == "19" {
                    Action::Reply(format!("ACK 19 {}", words[2]))
                } else {
                    admit(line)
                }
            },
            "verdict digest",
        ),
        (
            "malformed-verdict",
            |line| {
                let words: Vec<_> = line.split(' ').collect();
                if words[0] == "DONE" && words[1] == "19" {
                    Action::Reply(format!("ACK 19 {} {}", words[2], "Z".repeat(64)))
                } else {
                    admit(line)
                }
            },
            "verdict digest",
        ),
    ];
    for (name, respond, reason) in vectors {
        let campaign = Campaign::new(300, 300);
        let handle = supervisor(&campaign.path().join("gate"), respond);
        let error = performance::run(&campaign.options(name, true)).unwrap_err();
        stop(&campaign, handle);
        let text = error.to_string();
        assert!(
            text.contains(reason) && text.contains("analysis withheld"),
            "{name}: {text}"
        );
        // The partial bundle is retained but cannot be analyzed without a verdict.
        let directory = bundle(&campaign.path().join(name));
        let index = json(&directory.join("index.json"));
        assert!(index["gate"]["environment_sha256"].is_null(), "{name}");
        let transcript = json(&directory.join("gate-transcript.json"));
        assert!(
            transcript["termination"].as_str().unwrap().contains(reason),
            "{name}"
        );
        assert!(
            performance::verify(&directory, &campaign.recipe_sha256).is_err(),
            "{name}"
        );
    }
}

#[test]
fn rejected_profile_or_absent_supervisor_creates_no_bundle() {
    let campaign = Campaign::new(2000, 2000);
    let artifacts = campaign.path().join("artifacts");
    let mut options = campaign.options("artifacts", true);
    options.gate.as_mut().unwrap().profile_sha256 = "0".repeat(64);
    assert!(
        performance::run(&options)
            .unwrap_err()
            .to_string()
            .contains("profile identity")
    );
    // No supervisor holds the request FIFO open for reading.
    let error = performance::run(&campaign.options("artifacts", true)).unwrap_err();
    assert!(error.to_string().contains("not reading"), "{error}");
    // Release deadlines are fixed; self-test may shorten but never lengthen them.
    let longer = serde_json::to_vec(
        &serde_json::json!({"schema_version":1,"protocol":gate::PROTOCOL,
        "go_deadline_ms":660_001,"ack_deadline_ms":60_000,"environment":{}}),
    )
    .unwrap();
    assert!(gate::Profile::parse(&longer).is_err());
    let unknown = serde_json::to_vec(
        &serde_json::json!({"schema_version":1,"protocol":gate::PROTOCOL,
        "go_deadline_ms":1,"ack_deadline_ms":1,"environment":{},"extra":1}),
    )
    .unwrap();
    assert!(gate::Profile::parse(&unknown).is_err());
    // A regular file in place of a FIFO is refused before any slot.
    let fifo = campaign.path().join("gate").join(gate::REPLY_FIFO);
    fs::remove_file(&fifo).unwrap();
    File::create(&fifo).unwrap();
    let reader = OpenOptions::new()
        .read(true)
        .write(true)
        .open(campaign.path().join("gate").join(gate::REQUEST_FIFO))
        .unwrap();
    let error = performance::run(&campaign.options("artifacts", true)).unwrap_err();
    drop(reader);
    assert!(error.to_string().contains("FIFO"), "{error}");
    assert!(!artifacts.exists() || fs::read_dir(&artifacts).unwrap().next().is_none());
}

#[test]
fn gated_original_requires_gated_replay() {
    let campaign = Campaign::new(2000, 2000);
    let handle = supervisor(&campaign.path().join("gate"), admit);
    let report = performance::run(&campaign.options("original", true)).unwrap();
    stop(&campaign, handle);
    let replay = |artifacts: &str, gated: bool| {
        performance::replay(&ReplayOptions {
            directory: report.directory.clone(),
            recipe_sha256: campaign.recipe_sha256.clone(),
            artifacts: campaign.path().join(artifacts),
            campaign_id: [6; 16],
            gate: gated.then(|| campaign.gate()),
        })
    };
    let error = replay("ungated-replay", false).unwrap_err();
    assert!(error.to_string().contains("gated replay"), "{error}");
    let handle = supervisor(&campaign.path().join("gate"), admit);
    let replayed = replay("gated-replay", true).unwrap();
    stop(&campaign, handle);
    let index = json(&replayed.directory.join("index.json"));
    assert_eq!(index["schema_version"], 2);
    assert!(
        replayed
            .directory
            .join("original/gate-transcript.json")
            .is_file()
    );
    performance::verify(&replayed.directory, &replayed.recipe_sha256).unwrap();
}

/// Journal v1 records: kind u8, sequence u64, monotonic ns u64, request u64, length u32.
fn journal_times(path: &Path) -> Vec<(u8, u64, u64)> {
    let bytes = fs::read(path).unwrap();
    let mut rest = bytes.as_slice();
    let mut records = Vec::new();
    while !rest.is_empty() {
        let length = u32::from_be_bytes(rest[25..29].try_into().unwrap()) as usize;
        records.push((
            rest[0],
            u64::from_be_bytes(rest[9..17].try_into().unwrap()),
            u64::from_be_bytes(rest[17..25].try_into().unwrap()),
        ));
        rest = &rest[29 + length..];
    }
    records
}

#[test]
fn gate_wait_is_outside_external_invoke_to_observe() {
    let delay = Duration::from_millis(400);
    let campaign = Campaign::new(2000, 2000);
    let handle = supervisor(&campaign.path().join("gate"), move |line| {
        let words: Vec<_> = line.split(' ').collect();
        match (words[0], words[1]) {
            ("READY", "2") => Action::Reply(format!("ABORT 2 {}", words[2])),
            ("DONE", "2") => Action::Reply(format!("ACK 2 {} {VERDICT}", words[2])),
            ("READY", ordinal) => Action::Delayed(delay, format!("GO {ordinal} {}", words[2])),
            _ => admit(line),
        }
    });
    performance::run(&campaign.options("artifacts", true)).unwrap();
    stop(&campaign, handle);
    let directory = bundle(&campaign.path().join("artifacts"));
    let transcript = json(&directory.join("gate-transcript.json"));
    let lines = transcript["lines"].as_array().unwrap();
    let waited = lines[1]["ns"].as_u64().unwrap() - lines[0]["ns"].as_u64().unwrap();
    assert!(waited >= delay.as_nanos() as u64);
    let index = json(&directory.join("index.json"));
    for run in index["runs"].as_array().unwrap() {
        let Some(native) = run["directory"].as_str() else {
            continue;
        };
        // The native journal clock starts at session start, after GO was received.
        let records = journal_times(&directory.join(native).join("history.bin"));
        let invoke = records.iter().find(|r| r.0 == 1 && r.2 == 2).unwrap().1;
        let observe = records.iter().find(|r| r.0 == 3 && r.2 == 2).unwrap().1;
        assert!(observe - invoke < delay.as_nanos() as u64);
        assert!(records.iter().all(|r| r.1 < delay.as_nanos() as u64));
    }
}

#[test]
fn worker_nice_mismatch_is_a_slot_failure_and_stat_parsing_is_exact() {
    let stat = "42 (a) b (c)) S 1 42 42 0 -1 4194560 1 0 0 0 0 0 0 0 20 0 1 0 5 0";
    assert_eq!(gate::nice_field(stat).unwrap(), 0);
    let stat = "42 (worker) S 1 42 42 0 -1 4194560 1 0 0 0 0 0 0 0 21 1 1 0 5 0";
    assert_eq!(gate::nice_field(stat).unwrap(), 1);
    assert!(gate::nice_field("42 (worker) S 1").is_err());
    let campaign = Campaign::new(2000, 2000);
    let handle = supervisor(&campaign.path().join("gate"), |line| {
        let words: Vec<_> = line.split(' ').collect();
        match (words[0], words[1]) {
            ("READY", "2") => Action::Reply(format!("ABORT 2 {}", words[2])),
            ("DONE", "2") => Action::Reply(format!("ACK 2 {} {VERDICT}", words[2])),
            _ => admit(line),
        }
    });
    let path = campaign.path();
    // The worker inherits the controller's raised nice value.
    let status = std::process::Command::new("nice")
        .args(["-n", "1", env!("CARGO_BIN_EXE_xadbtest"), "run-performance"])
        .args(["--recipe".as_ref(), path.join("recipe.json").as_os_str()])
        .args(["--recipe-sha256", &campaign.recipe_sha256])
        .args([
            "--baseline-worker".as_ref(),
            path.join("worker").as_os_str(),
        ])
        .args([
            "--candidate-worker".as_ref(),
            path.join("worker").as_os_str(),
        ])
        .args([
            "--baseline-receipt".as_ref(),
            path.join("receipt.json").as_os_str(),
        ])
        .args([
            "--candidate-receipt".as_ref(),
            path.join("receipt.json").as_os_str(),
        ])
        .args([
            "--baseline-config".as_ref(),
            path.join("config.json").as_os_str(),
        ])
        .args([
            "--candidate-config".as_ref(),
            path.join("config.json").as_os_str(),
        ])
        .args(["--artifacts".as_ref(), path.join("artifacts").as_os_str()])
        .args(["--gate".as_ref(), path.join("gate").as_os_str()])
        .args([
            "--gate-profile".as_ref(),
            path.join("profile.json").as_os_str(),
        ])
        .args(["--gate-profile-sha256", &campaign.profile_sha256])
        .output()
        .unwrap();
    stop(&campaign, handle);
    assert_eq!(status.status.code(), Some(2), "{status:?}");
    let directory = bundle(&path.join("artifacts"));
    let index = json(&directory.join("index.json"));
    let failed: Vec<_> = index["runs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["ordinal"].as_u64().unwrap() < 2)
        .collect();
    assert_eq!(failed.len(), 2);
    for run in failed {
        assert!(
            run["failure"]
                .as_str()
                .unwrap()
                .contains("worker nice 1 is not 0")
        );
        let native = directory.join(run["directory"].as_str().unwrap());
        assert!(native.join("failure.json").is_file());
        assert!(!native.join("completion.json").exists());
    }
    // Partial option sets are refused rather than silently running ungated.
    let partial = std::process::Command::new(env!("CARGO_BIN_EXE_xadbtest"))
        .args(["run-performance", "--gate", "x"])
        .output()
        .unwrap();
    assert_eq!(partial.status.code(), Some(3));
}
