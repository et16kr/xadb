//! Explicit retained-output adapter for the changed diagnostic hook. Native
//! invocation is separate; no old performance campaign or worker is reexecuted.
use std::{fs, io::Cursor, path::PathBuf};
use test_controller::performance::{Side, check_output};
use test_protocol::{engine, frame, performance as wire};

fn check(case: &str) {
    let root = PathBuf::from(
        std::env::var_os("XADBTEST_WINDOW_NATIVE_EVIDENCE")
            .expect("explicit retained native evidence directory required"),
    );
    let build: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("build.json")).unwrap()).unwrap();
    let run = root.join(format!("k0-{case}/run"));
    let mut requests = Cursor::new(fs::read(run.join("input.bin")).unwrap());
    let mut responses = Cursor::new(fs::read(run.join("worker.stdout")).unwrap());
    let first: engine::Request = frame::read(&mut requests).unwrap().unwrap();
    let first_output: engine::Response = frame::read(&mut responses).unwrap().unwrap();
    assert_eq!(first.run, first_output.run);
    assert!(matches!(
        first_output.observation,
        engine::Observation::Ready {}
    ));
    let request: engine::Request = frame::read(&mut requests).unwrap().unwrap();
    let response: engine::Response = frame::read(&mut responses).unwrap().unwrap();
    assert_eq!(
        (request.run, request.request, request.actor),
        (response.run, response.request, response.actor)
    );
    assert!(
        frame::read::<engine::Request>(&mut requests)
            .unwrap()
            .is_none()
    );
    assert!(
        frame::read::<engine::Response>(&mut responses)
            .unwrap()
            .is_none()
    );
    let engine::Action::Performance { input } = request.action else {
        panic!("not performance")
    };
    let engine::Observation::Performance { output } = response.observation else {
        panic!("not performance")
    };
    let expected_build: wire::BuildIdentity =
        serde_json::from_value(build["build"].clone()).unwrap();
    let side = Side {
        worker_sha256: build["workers"]["k0"]["sha256"].as_str().unwrap().into(),
        receipt_sha256: String::new(),
        config_sha256: String::new(),
        build: expected_build,
        mode: wire::DiagnosticsMode::Off,
    };
    let suffix = match case {
        "active" => Some(".diagnostic_window.v1"),
        "prewarmup-continuous-v1" => Some(".diagnostic_prewarmup_continuous.v1"),
        "prewarmup-wait-v1" => Some(".diagnostic_prewarmup_wait.v1"),
        _ => None,
    };
    if let Some(suffix) = suffix {
        assert_eq!(output.path, format!("{}{suffix}", wire::PATH));
        assert_eq!(
            check_output(&input, &side, &output),
            Err("input/path/build identity mismatch")
        );
        let mut temporary = *output.clone();
        temporary.path = wire::PATH.into();
        check_output(&input, &side, &temporary).unwrap();
    } else {
        check_output(&input, &side, &output).unwrap();
    }
}
#[test]
#[ignore = "requires explicitly retained native window evidence"]
fn retained_diagnostic_output_is_correct_but_not_accepted_as_performance() {
    check("active");
}
#[test]
#[ignore = "requires explicitly retained native window evidence"]
fn retained_disabled_hook_preserves_original_output_semantics() {
    check("disabled");
}

#[test]
#[ignore = "requires explicitly retained native window evidence"]
fn retained_prewarmup_continuous_output_is_correct_and_not_accepted() {
    check("prewarmup-continuous-v1");
}

#[test]
#[ignore = "requires explicitly retained native window evidence"]
fn retained_prewarmup_wait_output_is_correct_and_not_accepted() {
    check("prewarmup-wait-v1");
}
