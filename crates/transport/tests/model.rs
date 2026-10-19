#![cfg(feature = "test-support")]
mod exploration;
mod support;
use exploration::{Event, Model, REQUIRED_WITNESSES, Sut};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    time::{Duration, Instant},
};

fn explore(depth: usize, require_witnesses: bool) {
    // Frozen safety budgets, not a success condition or state-space truncation.
    const MAX_STATES: usize = 360_000_000;
    const MAX_TRANSITIONS: u64 = 5_000_000_000;
    let started = Instant::now();
    let mut layer = vec![Vec::<Event>::new()];
    let mut states = 1;
    let mut transitions = 0u64;
    let mut witnesses = BTreeMap::new();
    for level in 0..depth {
        let mut next = Vec::new();
        let mut seen = HashSet::new();
        for trace in layer {
            let mut model = Model::default();
            for &event in &trace {
                model.step(event);
            }
            for event in model.enabled() {
                transitions += 1;
                assert!(
                    transitions <= MAX_TRANSITIONS
                        && states <= MAX_STATES
                        && started.elapsed() < Duration::from_secs(21_600),
                    "INCOMPLETE: model exploration budget exceeded"
                );
                let mut after = model.clone();
                after.step(event);
                let mut child = trace.clone();
                child.push(event);
                let actual = std::panic::catch_unwind(|| Sut::replay(&child));
                match actual {
                    Ok(()) => (),
                    Err(error) => {
                        eprintln!("COMM_MODEL_COUNTEREXAMPLE {child:?}");
                        std::panic::resume_unwind(error);
                    }
                }
                for name in after.witness(&model, event) {
                    witnesses.entry(name).or_insert_with(|| child.clone());
                }
                if seen.insert(after.packed_key()) {
                    states += 1;
                    assert!(
                        states <= MAX_STATES,
                        "INCOMPLETE: model state budget exceeded"
                    );
                    if level + 1 < depth {
                        next.push(child);
                    }
                }
            }
        }
        println!(
            "COMM_MODEL_LAYER depth={} states={} total_states={} transitions={} elapsed_ms={}",
            level + 1,
            seen.len(),
            states,
            transitions,
            started.elapsed().as_millis()
        );
        layer = next;
    }
    if require_witnesses {
        for name in REQUIRED_WITNESSES {
            assert!(
                witnesses.contains_key(name),
                "required witness unreachable: {name}"
            );
        }
    }
    assert!(
        states <= MAX_STATES
            && transitions <= MAX_TRANSITIONS
            && started.elapsed() < Duration::from_secs(21_600),
        "INCOMPLETE: final model budget exceeded"
    );
    for (name, trace) in &witnesses {
        println!("COMM_MODEL_WITNESS {name} {trace:?}");
    }
    println!(
        "COMM_MODEL_COMPLETE depth={depth} states={states} transitions={transitions} witnesses={}",
        witnesses.len()
    );
}

#[test]
fn independent_model_depth_two_smoke_checks_every_enabled_transition() {
    explore(2, false);
}

#[test]
fn canonical_time_and_terminal_keys_preserve_enabled_events_and_successors() {
    let mut layer = vec![Model::default()];
    for _ in 0..3 {
        let mut representatives: HashMap<_, Model> = HashMap::new();
        for model in layer {
            for event in model.enabled() {
                let mut after = model.clone();
                after.step(event);
                let key = after.packed_key();
                if let Some(previous) = representatives.get(&key) {
                    assert_eq!(
                        previous.state_key(),
                        after.state_key(),
                        "packed key collision"
                    );
                    assert_eq!(previous.enabled(), after.enabled());
                    for next in after.enabled() {
                        let mut a = previous.clone();
                        let mut b = after.clone();
                        assert_eq!(
                            a.step(next),
                            b.step(next),
                            "canonical key changed observation"
                        );
                        assert_eq!(
                            a.packed_key(),
                            b.packed_key(),
                            "canonical key changed successor"
                        );
                    }
                } else {
                    representatives.insert(key, after);
                }
            }
        }
        layer = representatives.into_values().collect();
    }
}

#[test]
#[ignore = "V3 exhaustive campaign; run with --release --ignored --nocapture"]
fn two_connections_two_slots_depth_eight_exhaustive_model() {
    explore(8, true);
}
