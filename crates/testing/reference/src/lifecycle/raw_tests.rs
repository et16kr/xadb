//! Independent flat1533 mapping and malformed raw descriptor tests.

use super::raw_action::{PrepareChoice, RawAction, RawActionError, action_at, index_of};
use super::raw_test_support::{byte, group_cases, run_256k, select, step, text, word};
use super::schema::{Api, Autocommit, Shape};
use super::target_selector::TargetSelector;
use serde_json::Value;

fn api(value: &Value) -> Api {
    match text(value) {
        "Prepare" => Api::Prepare,
        "BindAll" => Api::BindAll,
        "Execute" => Api::Execute,
        "Fetch" => Api::Fetch,
        "CloseResult" => Api::CloseResult,
        "CloseStatement" => Api::CloseStatement,
        "Begin" => Api::Begin,
        "SetAutocommit" => Api::SetAutocommit,
        "CommitAtomic" => Api::CommitAtomic,
        "Rollback" => Api::Rollback,
        "CloseSession" => Api::CloseSession,
        "ReopenSession" => Api::ReopenSession,
        "InspectStatement" => Api::InspectStatement,
        "InspectSession" => Api::InspectSession,
        "SignalContext" => Api::SignalContext,
        "ParkFetch" => Api::ParkFetch,
        "ResumeFetch" => Api::ResumeFetch,
        "BeginCommit" => Api::BeginCommit,
        "CommitFact" => Api::CommitFact,
        "FinishCommit" => Api::FinishCommit,
        "RecoveryConfirmedFreshEngine" => Api::RecoveryConfirmedFreshEngine,
        other => panic!("unknown literal API {other}"),
    }
}

fn action(value: &Value) -> RawAction {
    assert_eq!(value.as_object().expect("action object").len(), 6);
    let selector = (!value["selector"].is_null()).then(|| {
        let v = &value["selector"];
        match text(&v["kind"]) {
            "Issued" => TargetSelector::Issued(byte(&v["index"])),
            "FrozenUnissued" => TargetSelector::FrozenUnissued(byte(&v["index"])),
            other => panic!("unknown selector {other}"),
        }
    });
    let prepare = (!value["prepare"].is_null()).then(|| {
        let v = &value["prepare"];
        PrepareChoice {
            slot: byte(&v["slot"]),
            shape: match text(&v["shape"]) {
                "Select" => Shape::Select,
                "Dml" => Shape::Dml,
                other => panic!("shape {other}"),
            },
            parameter_count: byte(&v["parameter_count"]),
        }
    });
    let autocommit = (!value["autocommit"].is_null()).then(|| match text(&value["autocommit"]) {
        "On" => Autocommit::On,
        "Off" => Autocommit::Off,
        other => panic!("mode {other}"),
    });
    RawAction {
        api: api(&value["api"]),
        actor: byte(&value["actor"]),
        selector,
        prepare,
        autocommit,
        outcome: byte(&value["outcome"]),
    }
}

#[test]
fn raw_exact_mapping() {
    let cases: Vec<_> = group_cases("raw1533")
        .iter()
        .map(|v| {
            (
                text(&v["id"]).to_owned(),
                word(&v["index"]),
                action(&v["expected_action"]),
                word(&v["expected_inverse"]),
            )
        })
        .collect();
    let ids: Vec<_> = cases.iter().map(|c| c.0.clone()).collect();
    let selected = select("raw1533", &ids, &["action_at", "index_of"]);
    run_256k("raw1533", move || {
        for call in &selected {
            let (id, index, expected, inverse) = &cases[call.case];
            step("raw1533", id, call.direction, || match call.direction {
                "action_at" => assert_eq!(
                    action_at(*index),
                    Ok(*expected),
                    "{id}: complete independent descriptor"
                ),
                "index_of" => assert_eq!(
                    index_of(expected),
                    Ok(*inverse),
                    "{id}: inverse uses literal input, not prior output"
                ),
                _ => unreachable!(),
            });
        }
        println!("GROUP_COMPLETED group=raw1533 api_calls={}", selected.len());
    });
}

#[test]
fn raw_invalid_indices() {
    let cases: Vec<_> = group_cases("invalid_index")
        .iter()
        .map(|v| {
            (
                text(&v["id"]).to_owned(),
                word(&v["index"]),
                RawActionError::OutOfRange {
                    index: word(&v["expected_error"]["OutOfRange"]["index"]),
                },
            )
        })
        .collect();
    let ids: Vec<_> = cases.iter().map(|c| c.0.clone()).collect();
    let selected = select("invalid_index", &ids, &["action_at"]);
    run_256k("invalid_index", move || {
        for call in &selected {
            let (id, index, expected) = &cases[call.case];
            step("invalid_index", id, call.direction, || {
                assert_eq!(action_at(*index), Err(*expected), "{id}")
            });
        }
        println!(
            "GROUP_COMPLETED group=invalid_index api_calls={}",
            selected.len()
        );
    });
}

#[test]
fn raw_invalid_fields() {
    let cases: Vec<_> = group_cases("invalid_fields")
        .iter()
        .map(|v| {
            assert_eq!(text(&v["expected_error"]), "InvalidFields");
            (text(&v["id"]).to_owned(), action(&v["action"]))
        })
        .collect();
    let ids: Vec<_> = cases.iter().map(|c| c.0.clone()).collect();
    let selected = select("invalid_fields", &ids, &["index_of"]);
    run_256k("invalid_fields", move || {
        for call in &selected {
            let (id, input) = &cases[call.case];
            step("invalid_fields", id, call.direction, || {
                assert_eq!(index_of(input), Err(RawActionError::InvalidFields), "{id}")
            });
        }
        println!(
            "GROUP_COMPLETED group=invalid_fields api_calls={}",
            selected.len()
        );
    });
}
