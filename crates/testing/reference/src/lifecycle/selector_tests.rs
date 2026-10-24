//! Exact target selectors with whole logical-state immutability observations.

use super::literal::{Literals, SOURCE};
use super::raw_test_support::{group_cases, run_256k, select, step};
use super::selector_literal::SelectorCase;
use super::target_selector::resolve_selector;
use super::workspace::Workspace;
use sha2::{Digest, Sha256};

fn run_group(group: &'static str) {
    // JSON parsing, typed record construction, selection and native allocation
    // precede the 256KiB worker. Two heap workspaces are reused for every call.
    let cases: Vec<_> = group_cases(group)
        .into_iter()
        .map(SelectorCase::read)
        .collect();
    let ids: Vec<_> = cases.iter().map(|c| c.id.clone()).collect();
    let selected = select(group, &ids, &["resolve_selector"]);
    let mut actual = Workspace::try_new().unwrap();
    let mut expected = Workspace::try_new().unwrap();
    if group == "selector_baseline" {
        assert!(cases.iter().all(|c| c.read_set.is_none()));
        assert_eq!(
            format!("{:x}", Sha256::digest(SOURCE.as_bytes())),
            "ec4a7940a457882c155eb84e2d9e5602eaf880563bab4f6f04d0b643bcec42d2"
        );
        let literals = Literals::new().unwrap();
        literals.state("initial", actual.state_mut()).unwrap();
        literals.state("initial", expected.state_mut()).unwrap();
    } else {
        assert!(cases.iter().all(|c| c.read_set.is_some()));
    }
    run_256k(group, move || {
        for call in &selected {
            let case = &cases[call.case];
            if let Some(input) = &case.read_set {
                input.put(actual.state_mut());
                input.put(expected.state_mut());
            }
            step(group, &case.id, call.direction, || {
                let response =
                    resolve_selector(actual.state(), case.actor, case.kind, case.selector);
                // This is an effect observation, not full-state invariant
                // validation. Derived equality covers every logical scalar,
                // initialized record and membership field, including unread tables.
                assert_eq!(
                    actual.state(),
                    expected.state(),
                    "{}: state changed",
                    case.id
                );
                assert_eq!(
                    response, case.expected,
                    "{}: exact selector result",
                    case.id
                );
            });
        }
        println!("GROUP_COMPLETED group={group} api_calls={}", selected.len());
    });
}

#[test]
fn selector_baseline() {
    run_group("selector_baseline");
}

#[test]
fn selector_history() {
    run_group("selector_history");
}

#[test]
fn selector_namespace() {
    run_group("selector_namespace");
}

#[test]
fn selector_local_errors() {
    run_group("local_errors");
}

#[test]
fn selector_readset_controls() {
    run_group("readset_control");
}
