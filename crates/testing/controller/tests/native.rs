use std::{
    fs,
    path::{Path, PathBuf},
};
use test_controller::{
    file_sha256,
    native::{ErrorKind, Options, Session},
};
use test_protocol::{
    engine::{Action, Observation, Request, Response},
    frame,
};

fn options(root: &Path, mode: &str) -> Options {
    let binary = std::env::var_os("XADBTEST_NATIVE_MUTANT_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_xadbtest-native-mutant-worker").into());
    let mut options = Options::new(
        binary.clone(),
        file_sha256(&binary).unwrap(),
        std::env::var_os("XADBTEST_NATIVE_MUTANT_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.to_path_buf()),
        [9; 16],
    );
    options.test.worker_args = vec![mode.into()];
    options.test.limits.request_ms = 500;
    options.test.limits.run_ms = 2000;
    options.test.limits.drain_ms = 100;
    options
}
fn session(root: &Path, mode: &str) -> Session {
    Session::start(options(root, mode)).unwrap()
}

// Independent reader for the documented journal header, never worker-produced evidence.
fn events(path: &Path) -> Vec<(u8, u64, Vec<u8>)> {
    let bytes = fs::read(path).unwrap();
    let mut offset = 0;
    let mut result = Vec::new();
    let mut previous_time = 0;
    while offset != bytes.len() {
        let header = &bytes[offset..offset + 29];
        assert_eq!(
            u64::from_be_bytes(header[1..9].try_into().unwrap()),
            result.len() as u64
        );
        let time = u64::from_be_bytes(header[9..17].try_into().unwrap());
        assert!(time >= previous_time);
        previous_time = time;
        let request = u64::from_be_bytes(header[17..25].try_into().unwrap());
        let length = u32::from_be_bytes(header[25..29].try_into().unwrap()) as usize;
        offset += 29;
        result.push((header[0], request, bytes[offset..offset + length].to_vec()));
        offset += length;
    }
    result
}

#[test]
fn external_journal_orders_invoke_sent_observe_and_retains_exact_worker() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session(root.path(), "correct");
    let path = session.directory().to_path_buf();
    let id = session.invoke(0, Action::Metrics {}).unwrap();
    let response = session.receive().unwrap();
    assert_eq!(response.request, id);
    assert_eq!(response.observation, Observation::Ready {});
    let recorded = events(&path.join("history.bin"));
    assert_eq!(
        recorded.iter().map(|e| (e.0, e.1)).collect::<Vec<_>>(),
        [(1, 1), (2, 1), (3, 1)]
    );
    let mut invoke = recorded[0].2.as_slice();
    let request = frame::read::<Request>(&mut invoke).unwrap().unwrap();
    assert_eq!(request.action, Action::Metrics {});
    let mut observe = recorded[2].2.as_slice();
    assert_eq!(
        frame::read::<Response>(&mut observe).unwrap(),
        Some(response)
    );
    let report = session.finish().unwrap();
    assert_eq!((report.requests, report.observations), (1, 1));
    assert_eq!(
        report.worker_sha256,
        file_sha256(&path.join("worker")).unwrap()
    );
    assert_eq!(
        report.history_sha256,
        file_sha256(&path.join("history.bin")).unwrap()
    );
    assert_eq!(events(&path.join("history.bin")).last().unwrap().0, 5);
    assert!(path.join("completion.json").is_file());
}

#[test]
fn reversed_responses_match_two_actors_and_full_32_pending_boundary() {
    let root = tempfile::tempdir().unwrap();
    for (mode, count) in [("reverse", 2), ("queue32", 32)] {
        let mut session = session(root.path(), mode);
        for actor in 0..count {
            assert_eq!(
                session.invoke(actor, Action::Metrics {}).unwrap(),
                u64::from(actor) + 1
            );
        }
        for actor in (0..count).rev() {
            let response = session.receive().unwrap();
            assert_eq!(
                (response.actor, response.request),
                (actor, u64::from(actor) + 1)
            );
        }
        assert_eq!(session.finish().unwrap().observations, u64::from(count));
    }
}

#[test]
fn identity_duplicate_partial_oversized_malformed_and_unsolicited_responses_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    for mode in [
        "wrong-run",
        "wrong-request",
        "wrong-actor",
        "wrong-version",
        "duplicate",
        "partial",
        "oversized",
        "malformed",
        "unsolicited",
        "nonzero",
    ] {
        let mut session = session(root.path(), mode);
        let path = session.directory().to_path_buf();
        let result = session
            .invoke(0, Action::Metrics {})
            .and_then(|_| session.receive());
        let error = match result {
            Ok(_) => session.finish().unwrap_err(),
            Err(error) => error,
        };
        assert_eq!(error.kind, ErrorKind::Protocol, "{mode}: {error}");
        assert!(path.join("failure.json").is_file());
        assert!(!path.join("completion.json").exists());
    }
}

#[test]
fn request_timeout_and_stdin_backpressure_do_not_block_cleanup() {
    let root = tempfile::tempdir().unwrap();
    for mode in ["missing", "late", "never-read"] {
        let mut options = options(root.path(), mode);
        options.test.limits.request_ms = 30;
        let mut session = Session::start(options).unwrap();
        let action = if mode == "never-read" {
            Action::Lookup {
                snapshot: 1,
                table: 1,
                name: "a".repeat(1024 * 1024),
            }
        } else {
            Action::Metrics {}
        };
        let result = session.invoke(0, action).and_then(|_| session.receive());
        let error = result.unwrap_err();
        assert_eq!(error.kind, ErrorKind::Timeout, "{mode}");
        assert_eq!(session.invoke(1, Action::Metrics {}).unwrap_err(), error);
    }
}

#[test]
fn early_response_before_complete_large_request_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session(root.path(), "premature");
    let error = session
        .invoke(
            0,
            Action::Lookup {
                snapshot: 1,
                table: 1,
                name: "a".repeat(1024 * 1024),
            },
        )
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Protocol);
}

#[test]
fn bounded_output_error_and_input_budgets_are_not_silent_truncation() {
    let root = tempfile::tempdir().unwrap();
    for (mode, stream) in [("large-response", 0), ("stderr-flood", 1), ("correct", 2)] {
        let mut options = options(root.path(), mode);
        match stream {
            0 => options.test.limits.stdout_bytes = 512,
            1 => options.test.limits.stderr_bytes = 512,
            _ => options.test.limits.input_bytes = 1,
        }
        let mut session = Session::start(options).unwrap();
        let error = session
            .invoke(0, Action::Metrics {})
            .and_then(|_| session.receive())
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Limit, "{mode}");
    }
}

#[test]
fn journal_failure_before_send_or_observe_stays_failed_even_when_cleanup_also_fails() {
    let root = tempfile::tempdir().unwrap();
    for at in [0, 2] {
        let mut options = options(root.path(), "correct");
        options.test.fail_journal_at_event = Some(at);
        options.test.fail_cleanup = true;
        let mut session = Session::start(options).unwrap();
        let path = session.directory().to_path_buf();
        let error = session
            .invoke(0, Action::Metrics {})
            .and_then(|_| session.receive())
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Io);
        assert!(error.cleanup.as_ref().unwrap().contains("cleanup"));
        assert_eq!(events(&path.join("history.bin")).len(), at as usize);
        assert_eq!(session.invoke(1, Action::Metrics {}).unwrap_err(), error);
        assert!(!path.join("completion.json").exists());
    }
}

#[test]
fn journal_byte_limit_and_unconsumed_response_prevent_successful_close() {
    let root = tempfile::tempdir().unwrap();
    let mut options = options(root.path(), "correct");
    options.test.limits.journal_bytes = 1;
    let mut bounded = Session::start(options).unwrap();
    assert_eq!(
        bounded.invoke(0, Action::Metrics {}).unwrap_err().kind,
        ErrorKind::Limit
    );
    let mut pending = session(root.path(), "correct");
    pending.invoke(0, Action::Metrics {}).unwrap();
    assert_eq!(pending.finish().unwrap_err().kind, ErrorKind::Protocol);
}

#[test]
fn descendant_holding_streams_hits_drain_deadline_and_is_signalled() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session(root.path(), "descendant");
    let directory = session.directory().to_path_buf();
    session.invoke(0, Action::Metrics {}).unwrap();
    session.receive().unwrap();
    assert_eq!(session.finish().unwrap_err().kind, ErrorKind::Timeout);
    let stderr = fs::read_to_string(directory.join("stderr.bin")).unwrap();
    let pid: u32 = stderr
        .trim()
        .strip_prefix("DESCENDANT=")
        .unwrap()
        .parse()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let stat = fs::read_to_string(format!("/proc/{pid}/stat"));
        if stat
            .as_ref()
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
            || stat
                .as_ref()
                .is_ok_and(|s| s.split(") ").nth(1).unwrap().starts_with('Z'))
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "descendant still alive"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[test]
fn planned_kill_records_intent_and_confirmed_cleanup_after_response() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session(root.path(), "correct");
    let path = session.directory().to_path_buf();
    session.invoke(0, Action::Metrics {}).unwrap();
    session.receive().unwrap();
    session.kill_worker().unwrap();
    assert_eq!(
        events(&path.join("history.bin"))
            .iter()
            .map(|e| e.0)
            .collect::<Vec<_>>(),
        [1, 2, 3, 4, 6]
    );
    assert_eq!(
        session.invoke(0, Action::Metrics {}).unwrap_err().kind,
        ErrorKind::Closed
    );
}

#[test]
fn bad_worker_hash_and_same_actor_pending_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let mut invalid = options(root.path(), "correct");
    invalid.expected_worker_sha256 = "0".repeat(64);
    assert_eq!(
        Session::start(invalid).err().unwrap().kind,
        ErrorKind::Identity
    );
    let mut pending = session(root.path(), "missing");
    pending.invoke(0, Action::Metrics {}).unwrap();
    assert_eq!(
        pending.invoke(0, Action::Metrics {}).unwrap_err().kind,
        ErrorKind::Protocol
    );
}

fn wait_marker(directory: &Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !directory.join("work/emitted").is_file() {
        assert!(
            std::time::Instant::now() < deadline,
            "worker did not reach emission cut"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[test]
fn valid_response_is_preserved_when_same_write_exceeds_stdout_budget() {
    let root = tempfile::tempdir().unwrap();
    let mut options = options(root.path(), "ack-then-overflow");
    let expected = Response {
        version: 1,
        run: [9; 16],
        request: 1,
        actor: 0,
        observation: Observation::Ready {},
    };
    let wire = frame::encode(&expected).unwrap();
    options.test.limits.stdout_bytes = wire.len();
    let mut session = Session::start(options).unwrap();
    let path = session.directory().to_path_buf();
    let result = session
        .invoke(0, Action::Metrics {})
        .and_then(|_| session.receive());
    let error = match result {
        Err(error) => error,
        Ok(_) => session.finish().unwrap_err(),
    };
    assert_eq!(error.kind, ErrorKind::Limit);
    let observed = events(&path.join("history.bin"));
    assert_eq!(observed.iter().map(|e| e.0).collect::<Vec<_>>(), [1, 2, 3]);
    assert_eq!(observed[2].2, wire);
    assert_eq!(fs::read(path.join("stdout.bin")).unwrap(), wire);
    assert!(!path.join("completion.json").exists());
}

#[test]
fn responses_present_before_invoke_cannot_be_retroactively_matched() {
    let root = tempfile::tempdir().unwrap();
    for mode in ["before-invoke", "before-invoke-partial"] {
        let mut session = session(root.path(), mode);
        let path = session.directory().to_path_buf();
        wait_marker(&path);
        assert_eq!(
            session.invoke(0, Action::Metrics {}).unwrap_err().kind,
            ErrorKind::Protocol
        );
        assert!(events(&path.join("history.bin")).is_empty());
    }
}

#[test]
fn partial_response_start_cannot_bind_a_later_request_identity() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session(root.path(), "partial-next-request");
    let path = session.directory().to_path_buf();
    session.invoke(0, Action::Metrics {}).unwrap();
    wait_marker(&path);
    let result = session
        .invoke(1, Action::Metrics {})
        .and_then(|_| session.receive());
    assert_eq!(result.unwrap_err().kind, ErrorKind::Protocol);
    assert!(events(&path.join("history.bin")).iter().all(|e| e.0 != 3));
}

#[test]
fn admission_drains_more_than_one_quantum_before_registering_new_request() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session(root.path(), "queued-next-request");
    let path = session.directory().to_path_buf();
    session.invoke(0, Action::Metrics {}).unwrap();
    wait_marker(&path);
    assert_eq!(
        session.invoke(1, Action::Metrics {}).unwrap_err().kind,
        ErrorKind::Protocol
    );
    let observed = events(&path.join("history.bin"));
    assert_eq!(
        observed
            .iter()
            .filter(|e| e.0 == 3)
            .map(|e| e.1)
            .collect::<Vec<_>>(),
        [1]
    );
    assert_eq!(
        observed
            .iter()
            .filter(|e| e.0 == 1)
            .map(|e| e.1)
            .collect::<Vec<_>>(),
        [1]
    );
}
