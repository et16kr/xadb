use test_protocol::{
    engine::{Action, Observation, Request, Response},
    performance::{DiagnosticsMode, Input},
};
use test_reference::catalog::{CatalogInputBudget, CatalogResult, check_catalog};

#[test]
fn catalog_oracle_never_qualifies_performance_actions() {
    let request = Request {
        version: 1,
        run: [1; 16],
        request: 1,
        actor: 0,
        action: Action::Performance {
            input: Input {
                version: 1,
                seed: 0,
                row_count: 1,
                pin_old: false,
                mode: DiagnosticsMode::Off,
                capacity: 0,
                warmup: 4096,
                measure: 65536,
                latency_stride: 16,
            },
        },
    };
    let response = Response {
        version: 1,
        run: [1; 16],
        request: 1,
        actor: 0,
        observation: Observation::Ready {},
    };
    let mut budget = CatalogInputBudget::default();
    assert!(budget.admit(&request, &response).is_err());
    assert!(matches!(
        check_catalog(&[(request, response)]),
        CatalogResult::Invalid { .. }
    ));
}

#[test]
fn catalog_oracle_rejects_performance_observation_for_catalog_action() {
    // This deliberately empty performance payload is not workload evidence:
    // the catalog boundary must reject the observation kind itself.
    use test_protocol::{engine::Metrics, performance::*};
    let metrics = Metrics {
        operations: 0,
        rows_read: 0,
        row_references_copied: 0,
        resident_bytes: 0,
        snapshot_pins: 0,
    };
    let diagnostics = DiagnosticsSnapshot {
        mode: DiagnosticsMode::Off,
        event_capacity: 0,
        logical_bytes: 0,
        operations: None,
        dropped_events: None,
        saturated: None,
    };
    let output = Output {
        version: 1,
        input: Input {
            version: 1,
            seed: 0,
            row_count: 1,
            pin_old: false,
            mode: DiagnosticsMode::Off,
            capacity: 0,
            warmup: 4096,
            measure: 65536,
            latency_stride: 16,
        },
        path: PATH.into(),
        build: BuildIdentity {
            source_sha256: "0".repeat(64),
            toolchain: "synthetic".into(),
            target: "synthetic".into(),
            profile: "release".into(),
            self_test: false,
        },
        setup_ns: 0,
        warmup: Phase {
            elapsed_ns: 0,
            completed: 0,
        },
        measurement: Phase {
            elapsed_ns: 0,
            completed: 0,
        },
        error: None,
        read_values: vec![],
        final_values: vec![],
        old_values: None,
        latency_ns: vec![],
        metrics_before: metrics.clone(),
        metrics_after: metrics,
        diagnostics_before: diagnostics.clone(),
        diagnostics_after: diagnostics,
        events: vec![],
        events_available: true,
        cpu_ns: None,
        process_peak_rss_bytes: None,
    };
    let request = Request {
        version: 1,
        run: [1; 16],
        request: 1,
        actor: 0,
        action: Action::Initialize {
            database: [2; 16],
            timeline: 1,
            epoch: 1,
            resident_bytes: 65536,
            snapshot_pins: 2,
        },
    };
    let response = Response {
        version: 1,
        run: [1; 16],
        request: 1,
        actor: 0,
        observation: Observation::Performance {
            output: Box::new(output),
        },
    };
    assert!(
        CatalogInputBudget::default()
            .admit(&request, &response)
            .is_err()
    );
    assert!(matches!(
        check_catalog(&[(request, response)]),
        CatalogResult::Invalid { .. }
    ));
}
