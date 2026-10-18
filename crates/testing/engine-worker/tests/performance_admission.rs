use xadbtest_engine_worker::Session;
use xadbtest_protocol::{
    engine::{Action, Observation, Request},
    performance::{DiagnosticsMode, Input},
};

fn input() -> Input {
    Input {
        version: 1,
        seed: 7,
        row_count: 1,
        pin_old: true,
        mode: DiagnosticsMode::Off,
        capacity: 0,
        warmup: 4096,
        measure: 65536,
        latency_stride: 16,
    }
}
fn request(id: u64, actor: u8, action: Action) -> Request {
    Request {
        version: 1,
        run: [9; 16],
        request: id,
        actor,
        action,
    }
}
fn initialized() -> Session {
    let mut session = Session::new();
    let ready = session
        .dispatch(request(
            1,
            0,
            Action::Initialize {
                database: [1; 16],
                timeline: 1,
                epoch: 1,
                resident_bytes: 64 * 1024 * 1024,
                snapshot_pins: 32,
            },
        ))
        .unwrap();
    assert_eq!(ready.observation, Observation::Ready {});
    session
}

#[test]
fn performance_requires_actor_zero_request_two_and_no_intermediate_request() {
    for (id, actor) in [(2, 1), (3, 0)] {
        let mut session = initialized();
        assert!(
            session
                .dispatch(request(id, actor, Action::Performance { input: input() }))
                .is_err()
        );
        assert!(session.dispatch(request(4, 0, Action::Metrics {})).is_err());
    }
    let mut session = initialized();
    session.dispatch(request(2, 0, Action::Metrics {})).unwrap();
    assert!(
        session
            .dispatch(request(3, 0, Action::Performance { input: input() }))
            .is_err()
    );
    let mut fresh = Session::new();
    assert!(
        fresh
            .dispatch(request(1, 0, Action::Performance { input: input() }))
            .is_err()
    );
}

#[test]
fn invalid_performance_profile_poison_is_permanent() {
    let mut session = initialized();
    let mut invalid = input();
    invalid.measure = 1;
    assert!(
        session
            .dispatch(request(2, 0, Action::Performance { input: invalid }))
            .is_err()
    );
    assert!(session.dispatch(request(3, 0, Action::Metrics {})).is_err());
}

#[test]
fn successful_performance_observation_closes_session_before_later_effects() {
    let mut session = initialized();
    let response = session
        .dispatch(request(2, 0, Action::Performance { input: input() }))
        .unwrap();
    let Observation::Performance { output } = response.observation else {
        panic!("performance observation")
    };
    assert_eq!(output.measurement.completed, 65536);
    assert_eq!(output.read_values.len(), 65536);
    assert_eq!(output.error, None);
    assert!(session.dispatch(request(3, 0, Action::Metrics {})).is_err());
}
