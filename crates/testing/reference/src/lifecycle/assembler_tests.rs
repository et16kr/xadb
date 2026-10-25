//! Independent full-input assembly cases, first executed in a 256KiB worker.
//!
//! Selection: `XADB_ASSEMBLY_CASE` runs one exact case ID; `XADB_ASSEMBLY_FROM_CASE` skips the
//! completed prefix before that ID. Both select within the named group only and are never
//! combined. A failing case stops its group and leaves the remaining suffix NOT_RUN.
use super::assembler_literal::{Case, Expected, GROUPS, correct, fixtures, state_name};
use super::input_assembler::assemble_input;
use super::input_schema::{RawIdentity, ResolvedInput};
use super::schema::State;
use std::collections::BTreeMap;

fn environment(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(value) => Some(value),
        Err(std::env::VarError::NotPresent) => None,
        Err(error) => panic!("invalid {name}: {error}"),
    }
}

/// Exact case selection inside one frozen group. A selected ID must exist in some group, so a
/// mistyped ID fails instead of passing with zero calls; other groups report SKIPPED.
fn select<'a>(group: &str, cases: &'a [Case]) -> &'a [Case] {
    let exact = environment("XADB_ASSEMBLY_CASE");
    let from = environment("XADB_ASSEMBLY_FROM_CASE");
    assert!(
        exact.is_none() || from.is_none(),
        "XADB_ASSEMBLY_CASE and XADB_ASSEMBLY_FROM_CASE are exclusive"
    );
    if let Some(id) = exact.as_ref().or(from.as_ref()) {
        let known = fixtures()
            .groups
            .iter()
            .any(|(_, all)| all.iter().any(|c| &c.id == id));
        assert!(known, "selected case {id} is not in any frozen group");
    }
    let position = |id: &str| cases.iter().position(|c| c.id == id);
    let selected = match (exact, from) {
        (Some(id), None) => match position(&id) {
            Some(i) => &cases[i..=i],
            None => &cases[..0],
        },
        (None, Some(id)) => match position(&id) {
            Some(i) => &cases[i..],
            None => &cases[..0],
        },
        _ => cases,
    };
    if selected.is_empty() {
        println!("SKIPPED group={group} reason=selection_in_other_group calls=0");
    } else {
        println!(
            "SELECTED group={group} cases={} frozen_group_cases={} stack_bytes=262144",
            selected.len(),
            cases.len()
        );
    }
    selected
}

fn render_identity(raw: &RawIdentity) -> String {
    let b = raw.0;
    let word = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    let tail = if b[3] == 0 && b[12..].iter().all(|&x| x == 0) {
        String::new()
    } else {
        format!(" raw={b:?}")
    };
    format!(
        "[{},{},{},{},{},{},{}]{tail}",
        b[0],
        b[1],
        b[2],
        word(4),
        word(6),
        word(8),
        word(10)
    )
}
fn render_option(raw: &Option<RawIdentity>) -> String {
    raw.as_ref().map_or("null".into(), render_identity)
}

/// One line per observable field; native padding and unused tails are not rendered.
fn render(input: &ResolvedInput) -> Vec<String> {
    let mut lines = vec![
        format!(
            "api={:?} actor={} outcome={}",
            input.api, input.actor, input.outcome
        ),
        format!(
            "prepare={},{},{} set_autocommit={}",
            input.prepare_slot, input.prepare_shape, input.prepare_n, input.set_autocommit
        ),
        format!("target={}", render_option(&input.target)),
        format!("context={}", render_option(&input.context)),
        format!(
            "control={:?},{},{}",
            input.original_deadline, input.cancel, input.deadline_reached
        ),
    ];
    for (i, v) in input.bind_values.as_slice().iter().enumerate() {
        lines.push(format!(
            "bind[{i}]={},{},{:?}",
            v.tag,
            v.byte_length,
            &v.bytes[..usize::from(v.byte_length.min(8))]
        ));
    }
    let Some(env) = &input.environment else {
        lines.push("environment=null".into());
        return lines;
    };
    lines.push(format!(
        "env kind={:?} fact={} work={},{} cleanup={} commit={}",
        env.kind,
        render_identity(&env.fact),
        env.work_kind,
        env.work_class,
        env.cleanup,
        env.commit_class
    ));
    lines.push(format!(
        "env transaction={} receipt={} anchor={} fresh_engine={}",
        render_option(&env.transaction),
        render_option(&env.receipt),
        render_option(&env.anchor),
        render_option(&env.fresh_engine)
    ));
    for (i, o) in env.obligations.as_slice().iter().enumerate() {
        lines.push(format!(
            "env obligation[{i}]={} {:?} {:?} count={}",
            render_identity(&o.obligation),
            o.known_decision,
            o.after_ownership_summary,
            o.associated_owner_count
        ));
    }
    for (i, o) in env.owners.as_slice().iter().enumerate() {
        lines.push(format!(
            "env owner[{i}]={} {:?} {:?} o={} before={} after={}",
            render_identity(&o.resource),
            o.resource_kind,
            o.after_disposition,
            render_option(&o.obligation),
            render_option(&o.before_holder),
            render_option(&o.after_holder)
        ));
    }
    lines
}

fn difference(expected: &ResolvedInput, actual: &ResolvedInput) -> String {
    let (e, a) = (render(expected), render(actual));
    let mut out = String::new();
    for line in &e {
        if !a.contains(line) {
            out.push_str(&format!("\n  - expected {line}"));
        }
    }
    for line in &a {
        if !e.contains(line) {
            out.push_str(&format!("\n  + actual   {line}"));
        }
    }
    out
}

/// Run one call and compare; the state must be unchanged for success and error alike.
fn check(
    case: &Case,
    state: &State,
    snapshot: &mut State,
    out: &mut ResolvedInput,
) -> Option<String> {
    snapshot.copy_from(state);
    let result = assemble_input(state, &case.action, out);
    if state != snapshot {
        return Some("state changed".into());
    }
    match (&case.expected, result) {
        (Expected::Input(expected), Ok(())) if **expected == *out => None,
        (Expected::Input(expected), Ok(())) => Some(difference(expected, out)),
        (Expected::Error(expected), Err(actual)) if *expected == actual => None,
        (expected, actual) => Some(format!(
            "expected {} got {actual:?}",
            match expected {
                Expected::Input(_) => "success".into(),
                Expected::Error(e) => format!("{e:?}"),
            }
        )),
    }
}

/// Prepare pre-states outside the worker, then make every selected first call in 256KiB.
fn run_group(group: &'static str) {
    let input = fixtures();
    let cases = &input
        .groups
        .iter()
        .find(|(name, _)| *name == group)
        .expect("known group")
        .1;
    let selected = select(group, cases);
    let mut states: BTreeMap<&str, Box<State>> = BTreeMap::new();
    for case in selected {
        let name = state_name(&case.pre_state);
        if !states.contains_key(name) {
            let mut state = Box::<State>::default();
            input
                .literals
                .state(name, &mut state)
                .unwrap_or_else(|e| panic!("pre-state {name}: {e}"));
            states.insert(name, state);
        }
    }
    let report_all = environment("XADB_ASSEMBLY_REPORT_ALL").is_some();
    let worker = std::thread::Builder::new()
        .name(format!("asm-{group}"))
        .stack_size(256 * 1024)
        .spawn(move || {
            let mut snapshot = Box::<State>::default();
            let mut out = Box::<ResolvedInput>::default();
            let mut failures = 0usize;
            for case in selected {
                let state = &states[state_name(&case.pre_state)];
                println!("START group={group} id={}", case.id);
                match check(case, state, &mut snapshot, &mut out) {
                    None => println!("COMPLETED group={group} id={}", case.id),
                    Some(why) => {
                        failures += 1;
                        if report_all {
                            println!("MISMATCH group={group} id={}{why}", case.id);
                            continue;
                        }
                        println!("FAIL group={group} id={} remaining=NOT_RUN", case.id);
                        panic!("{}: {why}", case.id);
                    }
                }
            }
            assert_eq!(failures, 0, "{failures} mismatches in {group}");
        })
        .expect("spawn selected 256KiB worker");
    if let Err(payload) = worker.join() {
        std::panic::resume_unwind(payload);
    }
}

#[test]
fn assembly_initial_raw() {
    run_group("initial");
}

#[test]
fn assembly_original_pairs() {
    // The 51 original occurrences link to the 30 unique pairs; they are not extra calls.
    let input = fixtures();
    let original = &input.original;
    let ids: Vec<&str> = GROUPS.iter().map(|g| g.0).collect();
    assert!(ids.contains(&"original"));
    let occurrences = original["all_51_occurrences"]
        .as_array()
        .expect("occurrences");
    assert_eq!(occurrences.len(), 51);
    let overlap: BTreeMap<&str, &str> = original["overlap_links"]
        .as_array()
        .expect("overlap")
        .iter()
        .map(|l| {
            (
                l["original_pair"].as_str().unwrap(),
                l["case"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(overlap.len(), 3);
    let all: Vec<&str> = input
        .groups
        .iter()
        .flat_map(|(_, cases)| cases.iter().map(|c| c.id.as_str()))
        .collect();
    for occurrence in occurrences {
        let pair = occurrence["unique_pair_id"].as_str().expect("pair");
        let linked = overlap
            .get(pair)
            .copied()
            .map_or(format!("asm/original/{pair}"), str::to_owned);
        assert!(
            all.contains(&linked.as_str()),
            "occurrence {pair} has no linked case"
        );
    }
    run_group("original");
}

#[test]
fn assembly_bind_payloads() {
    run_group("bind");
}

#[test]
fn assembly_boundaries() {
    run_group("boundary");
}

#[test]
fn assembly_rich_normal() {
    run_group("rich_normal");
}

#[test]
fn assembly_rich_continuation() {
    run_group("rich_continuation");
}

#[test]
fn assembly_rich_rest() {
    run_group("rich_rest");
}

#[test]
fn assembly_supplement() {
    run_group("supplement");
}

#[test]
fn assembly_supplement_second() {
    run_group("supplement2");
}

#[test]
fn assembly_supplement_third() {
    run_group("supplement3");
}

#[test]
fn assembly_correction_original_check() {
    // Control: an exact original logical hash replaces only that expectation.
    let original = serde_json::json!({"a": 1});
    let digest = {
        use sha2::{Digest, Sha256};
        format!("{:x}", Sha256::digest(original.to_string().as_bytes()))
    };
    let mut group = serde_json::json!({"cases": [{"id": "x", "expected_input": original}]});
    let mut correction = serde_json::json!({"corrections": [{
        "id": "x",
        "original_expected_logical_sha256": digest,
        "corrected_expected_input": {"b": 2}
    }]});
    assert_eq!(correct(&mut group, &correction), 1);
    assert_eq!(
        group["cases"][0]["expected_input"],
        serde_json::json!({"b": 2})
    );
    // Known bad: a changed original must not be silently replaced.
    correction["corrections"][0]["original_expected_logical_sha256"] = "00".into();
    let mut changed = serde_json::json!({"cases": [{"id": "x", "expected_input": {"a": 1}}]});
    let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        correct(&mut changed, &correction)
    }));
    assert!(rejected.is_err());
    assert_eq!(
        changed["cases"][0]["expected_input"],
        serde_json::json!({"a": 1})
    );
}
