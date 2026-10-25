use test_protocol::{
    engine::{Action, Request},
    frame,
    performance::{DiagnosticsMode, Input, LATENCY_STRIDE, MEASURE, VERSION, WARMUP},
};

fn input() -> Input {
    Input {
        version: VERSION,
        seed: u64::MAX,
        row_count: 128,
        pin_old: true,
        mode: DiagnosticsMode::Detailed,
        capacity: 256,
        warmup: WARMUP,
        measure: MEASURE,
        latency_stride: LATENCY_STRIDE,
    }
}

#[test]
fn performance_profile_is_fixed_before_fixture_execution() {
    let original = input();
    assert!(original.validate());
    for (field, value) in [
        ("version", 2),
        ("row_count", 127),
        ("capacity", 1025),
        ("warmup", 4095),
        ("measure", 65535),
        ("latency_stride", 0),
    ] {
        let mut json = serde_json::to_value(&original).unwrap();
        json[field] = value.into();
        assert!(
            !serde_json::from_value::<Input>(json).unwrap().validate(),
            "{field}"
        );
    }
    for capacity in [0, 1, 256, 1024] {
        assert!(
            Input {
                capacity,
                row_count: 1,
                ..original.clone()
            }
            .validate()
        );
    }
}

#[test]
fn performance_json_rejects_unknown_duplicate_and_unknown_mode() {
    let json = serde_json::to_string(&input()).unwrap();
    for invalid in [
        json.replacen('{', "{\"extra\":1,", 1),
        json.replacen('{', "{\"version\":1,", 1),
        json.replace("detailed", "unbounded"),
    ] {
        assert!(serde_json::from_str::<Input>(&invalid).is_err());
    }
}

#[test]
fn performance_action_roundtrip_preserves_maximum_seed_and_exact_input() {
    let request = Request {
        version: 1,
        run: [1; 16],
        request: 2,
        actor: 0,
        action: Action::Performance { input: input() },
    };
    let bytes = frame::encode(&request).unwrap();
    let mut decoder = frame::Decoder::<Request>::new();
    let (consumed, decoded) = decoder.feed(&bytes).unwrap();
    assert_eq!(consumed, bytes.len());
    assert_eq!(decoded.unwrap(), request);
}

fn maximum_output() -> test_protocol::performance::Output {
    use test_protocol::{engine::Metrics, performance::*};
    let metrics = Metrics {
        operations: u64::MAX,
        rows_read: u64::MAX,
        row_references_copied: u64::MAX,
        resident_bytes: 64 * 1024 * 1024,
        snapshot_pins: 32,
    };
    let diagnostics = DiagnosticsSnapshot {
        mode: DiagnosticsMode::Detailed,
        event_capacity: 1024,
        logical_bytes: 65536,
        operations: Some(
            [OperationCounts {
                started: u64::MAX,
                completed: u64::MAX,
                failed: u64::MAX,
            }; 11],
        ),
        dropped_events: Some(u64::MAX),
        saturated: Some(true),
    };
    Output {
        version: 1,
        input: input(),
        path: PATH.into(),
        build: BuildIdentity {
            source_sha256: "f".repeat(64),
            toolchain: "rustc 1.95.0".into(),
            target: "x86_64-unknown-linux-gnu".into(),
            profile: "release".into(),
            self_test: false,
        },
        setup_ns: u64::MAX,
        warmup: Phase {
            elapsed_ns: u64::MAX,
            completed: 4096,
        },
        measurement: Phase {
            elapsed_ns: u64::MAX,
            completed: 65536,
        },
        error: None,
        read_values: vec![i64::MIN; 65536],
        final_values: vec![i64::MIN; 128],
        old_values: Some(vec![i64::MIN; 128]),
        latency_ns: vec![u64::MAX; 4096],
        metrics_before: metrics.clone(),
        metrics_after: metrics,
        diagnostics_before: diagnostics.clone(),
        diagnostics_after: diagnostics,
        events: vec![
            Event {
                sequence: u64::MAX,
                operation: Operation::NextPage,
                error: Some("XADB-ENGINE-0009".into())
            };
            1024
        ],
        events_available: true,
        cpu_ns: Some(u64::MAX),
        process_peak_rss_bytes: Some(u64::MAX),
    }
}

#[test]
fn maximum_performance_observation_fits_frame_and_oversized_output_is_rejected() {
    use test_protocol::engine::{Observation, Response};
    let mut response = Response {
        version: 1,
        run: [1; 16],
        request: 2,
        actor: 0,
        observation: Observation::Performance {
            output: Box::new(maximum_output()),
        },
    };
    let bytes = frame::encode(&response).unwrap();
    assert!(bytes.len() < frame::MAX_FRAME_BYTES);
    assert_eq!(
        frame::read::<Response>(&mut bytes.as_slice())
            .unwrap()
            .unwrap(),
        response
    );
    let Observation::Performance { output } = &mut response.observation else {
        unreachable!()
    };
    output.read_values.resize(300000, i64::MIN);
    assert!(frame::encode(&response).is_err());
}

#[test]
fn performance_output_nested_unknown_and_duplicate_fields_are_rejected() {
    use test_protocol::performance::Output;
    let json = serde_json::to_string(&maximum_output()).unwrap();
    for invalid in [
        json.replacen("\"build\":{", "\"build\":{\"unknown\":0,", 1),
        json.replacen(
            "\"measurement\":{",
            "\"measurement\":{\"completed\":65536,",
            1,
        ),
        json.replacen(
            "\"diagnostics_after\":{",
            "\"diagnostics_after\":{\"mode\":\"detailed\",",
            1,
        ),
    ] {
        assert!(serde_json::from_str::<Output>(&invalid).is_err());
    }
}
