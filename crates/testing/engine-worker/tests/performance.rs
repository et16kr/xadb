use xadbtest_engine_worker::{Session, performance};
use xadbtest_protocol::{
    engine::{Action, Observation, Request},
    performance::{self as wire, DiagnosticsMode, Input, OperationCounts},
};

fn input(mode: DiagnosticsMode, capacity: u32, row_count: u32, pin_old: bool) -> Input {
    Input {
        version: 1,
        seed: u64::MAX,
        row_count,
        pin_old,
        mode,
        capacity,
        warmup: wire::WARMUP,
        measure: wire::MEASURE,
        latency_stride: wire::LATENCY_STRIDE,
    }
}

fn assert_observations(output: &wire::Output) {
    assert!(output.error.is_none(), "{:?}", output.error);
    assert_eq!(output.version, 1);
    assert_eq!(output.path, wire::PATH);
    assert_eq!(output.warmup.completed, 4096);
    assert_eq!(output.measurement.completed, 65536);
    assert_eq!(output.read_values.len(), 65536);
    assert_eq!(output.latency_ns.len(), 4096);
    assert_eq!(output.final_values.len(), output.input.row_count as usize);
    assert_eq!(output.build.source_sha256.len(), 64);
    assert!(!output.build.self_test);
    // Independent scalar state, without any engine identity/storage implementation.
    let mut state = output.input.seed;
    let mut final_values: Vec<i64> = (0..output.input.row_count).map(i64::from).collect();
    for _ in 0..4096 {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
    }
    for (index, observed) in output.read_values.iter().enumerate() {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        assert_eq!(*observed, state as i64);
        final_values[index % output.input.row_count as usize] = state as i64;
    }
    assert_eq!(output.final_values, final_values);
    assert_eq!(
        output.old_values,
        output
            .input
            .pin_old
            .then(|| (0..output.input.row_count).map(i64::from).collect())
    );
    assert_eq!(
        output.metrics_after.operations - output.metrics_before.operations,
        65536
    );
    assert_eq!(
        output.metrics_after.rows_read - output.metrics_before.rows_read,
        65536
    );
    assert_eq!(
        output.metrics_after.row_references_copied - output.metrics_before.row_references_copied,
        65536 * u64::from(output.input.row_count)
    );
    assert_eq!(
        output.metrics_before.snapshot_pins,
        u64::from(output.input.pin_old)
    );
    assert_eq!(
        output.metrics_after.snapshot_pins,
        u64::from(output.input.pin_old)
    );
    assert_eq!(
        output.metrics_before.resident_bytes,
        output.metrics_after.resident_bytes
    );
    assert!(output.events_available);
    let before = &output.diagnostics_before;
    let after = &output.diagnostics_after;
    assert_eq!(before.mode, output.input.mode);
    assert_eq!(after.mode, output.input.mode);
    assert_eq!(after.event_capacity, output.input.capacity);
    match output.input.mode {
        DiagnosticsMode::Off => {
            assert_eq!(after.operations, None);
            assert_eq!(after.logical_bytes, 0);
            assert_eq!(after.saturated, None);
        }
        DiagnosticsMode::Always | DiagnosticsMode::Detailed => {
            let before_counts = before.operations.unwrap();
            let after_counts = after.operations.unwrap();
            for index in 0..11 {
                let expected = if [3, 5, 8].contains(&index) { 65536 } else { 0 };
                assert_eq!(
                    OperationCounts {
                        started: after_counts[index].started - before_counts[index].started,
                        completed: after_counts[index].completed - before_counts[index].completed,
                        failed: after_counts[index].failed - before_counts[index].failed,
                    },
                    OperationCounts {
                        started: expected,
                        completed: expected,
                        failed: 0
                    }
                );
            }
            assert_eq!(after.saturated, Some(false));
        }
    }
    if output.input.mode == DiagnosticsMode::Detailed {
        assert_eq!(output.events.len(), output.input.capacity as usize);
        assert_eq!(
            after.dropped_events.unwrap() - before.dropped_events.unwrap(),
            3 * 65536 - u64::from(output.input.capacity)
        );
        for pair in output.events.windows(2) {
            assert!(pair[0].sequence < pair[1].sequence);
        }
        assert!(output.events.iter().all(|event| event.error.is_none()));
    } else {
        assert_eq!(after.dropped_events, None);
        assert!(output.events.is_empty());
    }
}

#[test]
fn real_fixture_matrix_preserves_all_read_values_old_roots_and_exact_measurement_cuts() {
    for row_count in [1, 128] {
        for pin_old in [false, true] {
            for mode in [
                DiagnosticsMode::Off,
                DiagnosticsMode::Always,
                DiagnosticsMode::Detailed,
            ] {
                for capacity in [0, 1, 256] {
                    let input = input(mode, capacity, row_count, pin_old);
                    let output = performance::run(&input).unwrap();
                    assert_eq!(output.input, input);
                    assert_observations(&output);
                }
            }
        }
    }
}

fn request(id: u64, actor: u8, action: Action) -> Request {
    Request {
        version: 1,
        run: [1; 16],
        request: id,
        actor,
        action,
    }
}
fn initialize() -> Action {
    Action::Initialize {
        database: [1; 16],
        timeline: 1,
        epoch: 1,
        resident_bytes: 64 * 1024 * 1024,
        snapshot_pins: 32,
    }
}

#[test]
fn native_dispatch_accepts_only_one_isolated_batch_and_output_fits_frame() {
    let mut session = Session::new();
    session.dispatch(request(1, 0, initialize())).unwrap();
    let response = session
        .dispatch(request(
            2,
            0,
            Action::Performance {
                input: input(DiagnosticsMode::Detailed, 256, 128, true),
            },
        ))
        .unwrap();
    let Observation::Performance { output } = &response.observation else {
        panic!("performance output")
    };
    assert_observations(output);
    assert!(serde_json::to_vec(&response).unwrap().len() < 4 * 1024 * 1024);
    assert!(session.dispatch(request(3, 0, Action::Metrics {})).is_err());
    assert!(
        session
            .dispatch(request(
                4,
                0,
                Action::Performance {
                    input: input(DiagnosticsMode::Off, 0, 1, false)
                }
            ))
            .is_err()
    );
}

#[test]
fn invalid_profile_or_position_is_fatal_without_running_a_batch() {
    let base = input(DiagnosticsMode::Off, 0, 1, false);
    for field in 0..6 {
        let mut invalid = base.clone();
        match field {
            0 => invalid.version = 2,
            1 => invalid.capacity = 1025,
            2 => invalid.row_count = 2,
            3 => invalid.warmup = 4095,
            4 => invalid.measure = 65535,
            _ => invalid.latency_stride = 15,
        }
        assert!(performance::run(&invalid).is_err());
    }
    for (id, actor) in [(2, 1), (3, 0)] {
        let mut session = Session::new();
        session.dispatch(request(1, 0, initialize())).unwrap();
        assert!(
            session
                .dispatch(request(
                    id,
                    actor,
                    Action::Performance {
                        input: base.clone()
                    }
                ))
                .is_err()
        );
        assert!(
            session
                .dispatch(request(id + 1, 0, Action::Metrics {}))
                .is_err()
        );
    }
}
