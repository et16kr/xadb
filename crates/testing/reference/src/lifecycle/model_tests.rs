//! Model transitions against the independent 29-edge/12-trace literals and the loader.
//!
//! Expected states and inputs come from the frozen literal reader; responses and edge
//! observations are read from the same hand-authored file. No model output becomes an
//! expectation. Every first call runs in a 256KiB worker.
use super::codec::{decode_state, encode_state};
use super::input_schema::ResolvedInput;
use super::literal::{Literals, SOURCE};
use super::model::{
    Check, Complete, ContextEvent, Edge, EdgeClass, PublicTransaction, Response, Rows, transition,
};
use super::schema::{
    Certainty, ClientKnowledge, CommitProgress, ContextStatus, Decision, Delivery, Diagnostic,
    EngineCause, Identity, IdentityKind, OriginCause, Phase, Records, Retry, ReturnCode, State,
    StatementEffect, WireIdentity,
};
use serde_json::Value;
use std::fmt::Debug;

/// Expand base/put deltas, `$value` and `@identity` references of one named literal.
fn named(source: &Value, section: &str, name: &str) -> Value {
    let node = &source[section][name];
    let mut value = match node.get("base") {
        Some(base) => {
            let mut v = named(source, section, base.as_str().expect("base"));
            if let Some(put) = node.get("put") {
                for (pointer, replacement) in put.as_object().expect("put") {
                    let (parent, field) = pointer.rsplit_once('/').expect("pointer");
                    let target = v.pointer_mut(parent).expect("parent");
                    target
                        .as_object_mut()
                        .expect("object parent")
                        .insert(field.to_owned(), replacement.clone());
                }
            }
            v
        }
        None => node.clone(),
    };
    expand(source, &mut value);
    value
}
fn expand(source: &Value, value: &mut Value) {
    match value {
        Value::String(s) if s.starts_with('@') => {
            *value = source["identities"][&s[1..]].clone();
        }
        Value::Array(items) => items.iter_mut().for_each(|v| expand(source, v)),
        Value::Object(map) if map.contains_key("$value") => {
            let name = map["$value"].as_str().expect("value name").to_owned();
            *value = source["values"][&name].clone();
            expand(source, value);
        }
        Value::Object(map) => map.values_mut().for_each(|v| expand(source, v)),
        _ => {}
    }
}

fn vocabulary<T: TryFrom<u8> + Debug>(value: &Value) -> T {
    let name = value.as_str().expect("vocabulary string");
    (0..=u8::MAX)
        .filter_map(|tag| T::try_from(tag).ok())
        .find(|v| format!("{v:?}") == name)
        .unwrap_or_else(|| panic!("unknown vocabulary {name}"))
}
fn identity(value: &Value) -> Identity {
    let p: Vec<u64> = value
        .as_array()
        .expect("identity")
        .iter()
        .map(|v| v.as_u64().expect("identity part"))
        .collect();
    Identity {
        kind: IdentityKind::try_from(p[0] as u8).expect("kind"),
        actor: p[1] as u8,
        slot: p[2] as u8,
        engine: p[3] as u16,
        session: p[4] as u16,
        nonce: p[5] as u16,
        generation: p[6] as u16,
    }
}
fn optional_identity(value: &Value) -> Option<Identity> {
    (!value.is_null()).then(|| identity(value))
}

fn diagnostic(value: &Value) -> Option<Diagnostic> {
    (!value.is_null()).then(|| Diagnostic {
        code: vocabulary::<ReturnCode>(&value["code"]),
        phase: vocabulary::<Phase>(&value["phase"]),
        effect: vocabulary::<StatementEffect>(&value["effect"]),
        certainty: vocabulary::<Certainty>(&value["certainty"]),
        retry: vocabulary::<Retry>(&value["retry"]),
        engine_cause: vocabulary::<EngineCause>(&value["engine_cause"]),
        origin_cause: vocabulary::<OriginCause>(&value["origin_cause"]),
    })
}

fn response(value: &Value) -> Response {
    let mut obligations = Records::default();
    for o in value["recovery_obligations"]
        .as_array()
        .expect("obligations")
    {
        obligations
            .try_push(identity(o))
            .expect("obligation capacity");
    }
    Response {
        code: vocabulary(&value["code"]),
        phase: vocabulary(&value["phase"]),
        statement_effect: vocabulary(&value["statement_effect"]),
        certainty: vocabulary(&value["certainty"]),
        retry: vocabulary(&value["retry"]),
        engine_cause: vocabulary(&value["engine_cause"]),
        origin_cause: vocabulary(&value["origin_cause"]),
        diagnostic: diagnostic(&value["diagnostic"]),
        rows: match &value["rows"] {
            Value::Number(n) => Rows::Count(n.as_u64().expect("rows") as u8),
            _ => Rows::NotApplicable,
        },
        complete: match &value["complete"] {
            Value::Bool(true) => Complete::Yes,
            Value::Bool(false) => Complete::No,
            _ => Complete::NotApplicable,
        },
        affected_count: value["affected_count"].as_u64().map(|n| n as u8),
        returned_statement: optional_identity(&value["returned_statement"]),
        returned_result: optional_identity(&value["returned_result"]),
        public_transaction: match value["public_transaction"].as_str().expect("public") {
            "Idle" => PublicTransaction::Idle,
            "Active" => PublicTransaction::Active,
            "RecoveryPending" => PublicTransaction::RecoveryPending,
            other => panic!("public transaction {other}"),
        },
        transaction_decision: vocabulary::<Decision>(&value["transaction_decision"]),
        client_knowledge: vocabulary::<ClientKnowledge>(&value["client_knowledge"]),
        commit_progress: vocabulary::<CommitProgress>(&value["commit_progress"]),
        completion: optional_identity(&value["completion"]),
        delivery: vocabulary::<Delivery>(&value["delivery"]),
        recovery_obligations: obligations,
    }
}

fn check(value: &Value) -> Check {
    match value {
        Value::Bool(true) => Check::Yes,
        Value::Bool(false) => Check::No,
        _ => Check::NotChecked,
    }
}

fn expected_edge(source: &Value, edge: &Value) -> Edge {
    let class = match edge["class"].as_str().expect("class") {
        "CheckedAdmitted" => EdgeClass::Admitted,
        "CheckedRejected" => EdgeClass::Rejected,
        "CheckedNoEffect" => EdgeClass::NoEffect,
        "CheckedNoContextIssue" => EdgeClass::NoContextIssue,
        "CheckedContinuation" => EdgeClass::Continuation,
        other => panic!("class {other}"),
    };
    let event = &edge["context_event"];
    let status = |v: &Value| -> Option<ContextStatus> {
        (!v.is_null()).then(|| vocabulary::<ContextStatus>(v))
    };
    let mut expanded_event = event.clone();
    expand(source, &mut expanded_event);
    let controls = &edge["controls"];
    let authenticated = edge
        .get("authenticated_targets")
        .map(|_| super::model::Authenticated::default());
    Edge {
        class,
        response: Some(response(&named(
            source,
            "responses",
            edge["response"].as_str().expect("response"),
        ))),
        context_event: (!event.is_null()).then(|| ContextEvent {
            identity: identity(&expanded_event["identity"]),
            before: status(&expanded_event["before"]),
            after: status(&expanded_event["after"]).expect("after"),
        }),
        controls: super::model::Controls {
            entry_cancel: check(&controls["entry_cancel"]),
            entry_deadline: check(&controls["entry_deadline"]),
            observed_cancel: controls["observed_cancel"].as_bool().expect("observed"),
            observed_deadline: controls["observed_deadline"].as_bool().expect("observed"),
        },
        consumed_parts: edge["consumed_fact_parts"].as_u64().expect("parts") as u8,
        authenticated,
    }
}

/// Run every literal edge once and compare the full observation and successor.
#[test]
fn model_literal_edges() {
    let source: Value = serde_json::from_str(SOURCE).expect("literal JSON");
    let literals = Literals::new().expect("literals");
    let names: Vec<String> = source["edges"]
        .as_object()
        .expect("edges")
        .keys()
        .cloned()
        .collect();
    let mut cases = Vec::new();
    for name in &names {
        let edge = &source["edges"][name];
        let mut pre = Box::<State>::default();
        literals
            .state(edge["pre"].as_str().unwrap(), &mut pre)
            .expect("pre");
        let mut post = Box::<State>::default();
        literals
            .state(edge["post"].as_str().unwrap(), &mut post)
            .expect("post");
        let input: ResolvedInput = literals
            .input(edge["input"].as_str().unwrap(), None)
            .expect("input");
        cases.push((name.clone(), pre, input, post, expected_edge(&source, edge)));
    }
    let report_all = std::env::var("XADB_MODEL_REPORT_ALL").is_ok();
    let worker = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || {
            let mut failures = 0;
            for (name, pre, input, expected_post, expected) in &cases {
                println!("START edge={name}");
                let mut post = Box::<State>::default();
                let mut edge = Edge::default();
                transition(pre, input, &mut post, &mut edge).expect("model transition");
                let mut why = Vec::new();
                if edge != *expected {
                    why.push(format!(
                        "edge\n  expected {expected:?}\n  actual   {edge:?}"
                    ));
                }
                if *post != **expected_post {
                    why.push(state_difference(expected_post, &post));
                }
                let mut bytes = vec![0u8; 16 * 1024];
                match encode_state(&post, &WireIdentity::default(), &mut bytes) {
                    Ok(n) => {
                        let mut back = Box::<State>::default();
                        if let Err(e) =
                            decode_state(&bytes[..n], &WireIdentity::default(), &mut back)
                        {
                            why.push(format!("loader {e:?}"));
                        }
                    }
                    Err(e) => why.push(format!("encode {e:?}")),
                }
                if why.is_empty() {
                    println!("COMPLETED edge={name}");
                } else {
                    failures += 1;
                    println!("MISMATCH edge={name}\n{}", why.join("\n"));
                    if !report_all {
                        panic!("{name} mismatched");
                    }
                }
            }
            assert_eq!(failures, 0);
        })
        .expect("worker");
    if let Err(p) = worker.join() {
        std::panic::resume_unwind(p);
    }
}

fn state_difference(expected: &State, actual: &State) -> String {
    let mut out = String::from("state");
    macro_rules! table {
        ($($field:ident),*) => {$(
            if expected.$field != actual.$field {
                out.push_str(&format!("\n  {}:\n    expected {:?}\n    actual   {:?}", stringify!($field), expected.$field.as_slice(), actual.$field.as_slice()));
            }
        )*};
    }
    if expected.next_nonce != actual.next_nonce {
        out.push_str(&format!(
            "\n  next_nonce expected {:?} actual {:?}",
            expected.next_nonce, actual.next_nonce
        ));
    }
    if expected.current_engine != actual.current_engine {
        out.push_str("\n  current_engine differs");
    }
    table!(
        engines,
        session_incarnations,
        actors,
        prepared_slots,
        issued_statements,
        issued_results,
        private_descriptors,
        contexts,
        transactions,
        obligations,
        bindings,
        receipts,
        owners,
        facts,
        fact_obligations,
        fact_owners
    );
    out
}

fn literal_case(
    literals: &Literals,
    source: &Value,
    name: &str,
) -> (ResolvedInput, Box<State>, Edge) {
    let edge = &source["edges"][name];
    let input = literals
        .input(edge["input"].as_str().unwrap(), None)
        .expect("input");
    let mut post = Box::<State>::default();
    literals
        .state(edge["post"].as_str().unwrap(), &mut post)
        .expect("post");
    (input, post, expected_edge(source, edge))
}

/// Replay the 12 traces (51 occurrences) from their initial states, chaining model successors.
#[test]
fn model_literal_traces() {
    let source: Value = serde_json::from_str(SOURCE).expect("literal JSON");
    let literals = Literals::new().expect("literals");
    let mut traces = Vec::new();
    for trace in source["traces"].as_array().expect("traces") {
        let mut initial = Box::<State>::default();
        literals
            .state(trace["initial"].as_str().unwrap(), &mut initial)
            .expect("initial");
        let steps: Vec<_> = trace["edges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                let name = e.as_str().unwrap().to_owned();
                let (input, post, edge) = literal_case(&literals, &source, &name);
                (name, input, post, edge)
            })
            .collect();
        traces.push((trace["id"].as_str().unwrap().to_owned(), initial, steps));
    }
    let worker = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || {
            let mut occurrences = 0;
            for (id, initial, steps) in &traces {
                let mut pre = Box::<State>::default();
                pre.copy_from(initial);
                for (name, input, expected_post, expected) in steps {
                    occurrences += 1;
                    println!("START trace={id} edge={name}");
                    let mut post = Box::<State>::default();
                    let mut edge = Edge::default();
                    transition(&pre, input, &mut post, &mut edge).expect("model transition");
                    assert_eq!(edge, *expected, "{id}/{name} edge");
                    assert!(*post == **expected_post, "{id}/{name} post");
                    pre.copy_from(&post);
                    println!("COMPLETED trace={id} edge={name}");
                }
            }
            assert_eq!(occurrences, 51);
        })
        .expect("worker");
    if let Err(p) = worker.join() {
        std::panic::resume_unwind(p);
    }
}

/// Each mandatory known-bad observation differs from the model's normal observation.
#[test]
fn model_known_bad_differs() {
    let mut source: Value = serde_json::from_str(SOURCE).expect("literal JSON");
    let mutants = source["mutants"].as_array().expect("mutants").clone();
    for m in &mutants {
        let id = m["id"].as_str().unwrap();
        let states = source["states"].as_object_mut().unwrap();
        states.insert(format!("bad:{id}"), m["bad_post"].clone());
        let responses = source["responses"].as_object_mut().unwrap();
        responses.insert(format!("bad:{id}"), m["bad_response"].clone());
    }
    let literals = Literals::from_json(&source.to_string()).expect("literals");
    let mut cases = Vec::new();
    for m in &mutants {
        let id = m["id"].as_str().unwrap().to_owned();
        let name = m["first_required_divergence_edge"].as_str().unwrap();
        let edge = &source["edges"][name];
        let mut pre = Box::<State>::default();
        literals
            .state(edge["pre"].as_str().unwrap(), &mut pre)
            .expect("pre");
        let input = literals
            .input(edge["input"].as_str().unwrap(), None)
            .expect("input");
        let mut bad_post = Box::<State>::default();
        literals
            .state(&format!("bad:{id}"), &mut bad_post)
            .expect("bad post");
        let bad_response = response(&named(&source, "responses", &format!("bad:{id}")));
        cases.push((id, pre, input, bad_post, bad_response));
    }
    let worker = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || {
            for (id, pre, input, bad_post, bad_response) in &cases {
                let mut post = Box::<State>::default();
                let mut edge = Edge::default();
                transition(pre, input, &mut post, &mut edge).expect("model transition");
                let normal = edge.response.expect("response");
                assert!(
                    normal != *bad_response || *post != **bad_post,
                    "{id} not distinguished"
                );
                println!(
                    "DETECTED mutant={id} response_differs={} post_differs={}",
                    normal != *bad_response,
                    *post != **bad_post
                );
            }
        })
        .expect("worker");
    if let Err(p) = worker.join() {
        std::panic::resume_unwind(p);
    }
}

/// Every raw action at every literal state assembles and transitions without a model error,
/// and every successor passes the canonical loader. This is a smoke census, not a checker.
#[test]
fn model_raw_census() {
    use super::input_assembler::assemble_input;
    use super::raw_action::action_at;
    let source: Value = serde_json::from_str(SOURCE).expect("literal JSON");
    let literals = Literals::new().expect("literals");
    let names: Vec<String> = source["states"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    let mut states = Vec::new();
    for name in &names {
        let mut s = Box::<State>::default();
        literals.state(name, &mut s).expect("state");
        states.push((name.clone(), s));
    }
    let report_all = std::env::var("XADB_MODEL_REPORT_ALL").is_ok();
    let worker = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || {
            let mut counts = std::collections::BTreeMap::<String, usize>::new();
            let mut failures = 0usize;
            let mut bytes = vec![0u8; 16 * 1024];
            let mut back = Box::<State>::default();
            let mut post = Box::<State>::default();
            let mut input = Box::<ResolvedInput>::default();
            for (name, pre) in &states {
                for raw in 0..super::raw_action::RAW_ACTION_COUNT {
                    let action = action_at(raw).expect("raw");
                    if let Err(e) = assemble_input(pre, &action, &mut input) {
                        *counts.entry(format!("assembly:{e:?}")).or_default() += 1;
                        continue;
                    }
                    let mut edge = Edge::default();
                    let why = match transition(pre, &input, &mut post, &mut edge) {
                        Err(e) => Some(format!("model {e:?}")),
                        Ok(()) => match encode_state(&post, &WireIdentity::default(), &mut bytes) {
                            Err(e) => Some(format!("encode {e:?}")),
                            Ok(n) => decode_state(&bytes[..n], &WireIdentity::default(), &mut back)
                                .err()
                                .map(|e| format!("loader {e:?}")),
                        },
                    };
                    match why {
                        None => *counts.entry(format!("{:?}", edge.class)).or_default() += 1,
                        Some(why) => {
                            failures += 1;
                            println!(
                                "FAIL state={name} raw={raw} api={:?} outcome={} {why}",
                                action.api, action.outcome
                            );
                            if !report_all {
                                panic!("raw census failure");
                            }
                        }
                    }
                }
            }
            println!("CENSUS {counts:?}");
            assert_eq!(failures, 0, "{failures} raw census failures");
        })
        .expect("worker");
    if let Err(p) = worker.join() {
        std::panic::resume_unwind(p);
    }
}

/// Deterministic walks of length six through state-changing admitted edges, checking that every
/// raw action at every visited state transitions without a model error and passes the loader.
#[test]
fn model_deep_walks() {
    use super::input_assembler::assemble_input;
    use super::raw_action::{RAW_ACTION_COUNT, action_at};
    let walks: u64 = std::env::var("XADB_MODEL_WALKS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(40);
    let literals = Literals::new().expect("literals");
    let mut initial = Box::<State>::default();
    literals.state("initial", &mut initial).expect("initial");
    let worker = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || {
            let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
            let mut next = move || {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                seed
            };
            let mut bytes = vec![0u8; 16 * 1024];
            let mut back = Box::<State>::default();
            let mut post = Box::<State>::default();
            let mut pre = Box::<State>::default();
            let mut input = Box::<ResolvedInput>::default();
            let mut counts = std::collections::BTreeMap::<String, usize>::new();
            let mut candidates = Vec::new();
            let mut path = Vec::new();
            for walk in 0..walks {
                pre.copy_from(&initial);
                path.clear();
                for depth in 0..6 {
                    candidates.clear();
                    for raw in 0..RAW_ACTION_COUNT {
                        let action = action_at(raw).expect("raw");
                        if assemble_input(&pre, &action, &mut input).is_err() {
                            continue;
                        }
                        let mut edge = Edge::default();
                        if let Err(e) = transition(&pre, &input, &mut post, &mut edge) {
                            panic!("walk={walk} depth={depth} raw={raw} {:?}/{} model {e:?} path={path:?}", action.api, action.outcome);
                        }
                        let n = encode_state(&post, &WireIdentity::default(), &mut bytes)
                            .unwrap_or_else(|e| panic!("walk={walk} depth={depth} raw={raw} {:?}/{} encode {e:?}", action.api, action.outcome));
                        if let Err(e) = decode_state(&bytes[..n], &WireIdentity::default(), &mut back) {
                            panic!("walk={walk} depth={depth} raw={raw} {:?}/{} loader {e:?}", action.api, action.outcome);
                        }
                        *counts.entry(format!("{:?}/{:?}", action.api, edge.class)).or_default() += 1;
                        if matches!(edge.class, EdgeClass::Admitted | EdgeClass::Continuation | EdgeClass::NoContextIssue)
                            && edge.consumed_parts != 0 || matches!(action.api, super::schema::Api::Prepare | super::schema::Api::Begin | super::schema::Api::SetAutocommit | super::schema::Api::BeginCommit | super::schema::Api::ParkFetch | super::schema::Api::ReopenSession | super::schema::Api::SignalContext | super::schema::Api::BindAll) && matches!(edge.class, EdgeClass::Admitted | EdgeClass::NoContextIssue)
                        {
                            candidates.push(raw);
                        }
                    }
                    if candidates.is_empty() {
                        break;
                    }
                    let raw = candidates[(next() % candidates.len() as u64) as usize];
                    let action = action_at(raw).expect("raw");
                    assemble_input(&pre, &action, &mut input).expect("assembled");
                    let mut edge = Edge::default();
                    transition(&pre, &input, &mut post, &mut edge).expect("chosen");
                    path.push(raw);
                    pre.copy_from(&post);
                }
            }
            println!("WALK_CALLS {counts:?}");
        })
        .expect("worker");
    if let Err(p) = worker.join() {
        std::panic::resume_unwind(p);
    }
}

/// Assemble and apply one raw sequence from the initial literal, returning each edge and state.
fn run_raw(raws: &[u16]) -> Vec<(Edge, Box<State>)> {
    use super::input_assembler::assemble_input;
    use super::raw_action::action_at;
    let literals = Literals::new().expect("literals");
    let mut pre = Box::<State>::default();
    literals.state("initial", &mut pre).expect("initial");
    let mut input = Box::<ResolvedInput>::default();
    let mut out = Vec::new();
    for &raw in raws {
        let action = action_at(raw).expect("raw");
        assemble_input(&pre, &action, &mut input)
            .unwrap_or_else(|e| panic!("assemble {raw}: {e:?}"));
        let mut post = Box::<State>::default();
        let mut edge = Edge::default();
        transition(&pre, &input, &mut post, &mut edge)
            .unwrap_or_else(|e| panic!("model {raw}: {e:?}"));
        let mut bytes = vec![0u8; 16 * 1024];
        let n = encode_state(&post, &WireIdentity::default(), &mut bytes)
            .unwrap_or_else(|e| panic!("encode {raw}: {e:?}"));
        let mut back = Box::<State>::default();
        decode_state(&bytes[..n], &WireIdentity::default(), &mut back)
            .unwrap_or_else(|e| panic!("loader {raw}: {e:?}"));
        pre.copy_from(&post);
        out.push((edge, post));
    }
    out
}

fn classes(steps: &[(Edge, Box<State>)]) -> Vec<EdgeClass> {
    steps.iter().map(|(e, _)| e.class).collect()
}

/// Fixed regression traces for families the literal set does not admit. Expectations are
/// hand-derived from the owner sources; they are not an independent checker.
#[test]
fn model_fixed_traces() {
    use super::model::Exclusion;
    use super::schema::{EngineAdmission, Settlement, Shell, StatementLife, TransactionLife};
    let worker = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            // v3 §3 recovery witness: CommitUnknown fence, two closes, exact recovery, fresh reopen.
            let s = run_raw(&[6, 409, 1310, 1314, 1528, 1319]);
            assert_eq!(
                classes(&s),
                [
                    EdgeClass::Admitted,
                    EdgeClass::Admitted,
                    EdgeClass::Admitted,
                    EdgeClass::Admitted,
                    EdgeClass::NoContextIssue,
                    EdgeClass::NoContextIssue
                ]
            );
            let fenced = &s[1].1;
            assert_eq!(
                fenced.engines.as_slice()[0].admission,
                EngineAdmission::Fenced
            );
            assert_eq!(
                fenced.transactions.as_slice()[0].decision,
                Decision::Unknown
            );
            let recovered = &s[4].1;
            assert_eq!(recovered.current_engine.0, 2);
            assert_eq!(
                recovered.engines.as_slice()[0].admission,
                EngineAdmission::RecoveryConfirmedRetired
            );
            assert_eq!(
                recovered.obligations.as_slice()[0].settlement,
                Settlement::RecoveryConfirmed
            );
            let reopened = &s[5].1;
            let actor = reopened.actors.as_slice()[0];
            assert_eq!(
                (actor.shell, actor.engine.0, actor.session.0),
                (Shell::Open, 2, 3)
            );

            // ENV-FENCED-CLOSE: a close on a fenced engine cannot claim a new undo failure.
            let s = run_raw(&[6, 410, 1311]);
            assert_eq!(s[2].0.class, EdgeClass::Excluded(Exclusion::FencedClose));
            assert_eq!(
                s[1].1.transactions.as_slice()[0].decision,
                Decision::Committed
            );

            // Peer fence on a parked BeginCommit: only the exact NCR handoff may end it.
            let s = run_raw(&[1267, 1506, 1264, 1303, 1524]);
            assert_eq!(s[4].0.class, EdgeClass::Excluded(Exclusion::PeerFence));
            let s = run_raw(&[1267, 1506, 1264, 1303, 1525]);
            assert_eq!(s[4].0.class, EdgeClass::Continuation);
            let response = s[4].0.response.expect("response");
            assert_eq!(response.code, ReturnCode::RecoveryRequired);
            assert_eq!(response.engine_cause, EngineCause::PeerFence);

            // Atomic commit of a changed explicit RW transaction.
            let s = run_raw(&[1264, 6, 400, 1282]);
            assert_eq!(s[3].0.class, EdgeClass::Admitted);
            let t = s[3].1.transactions.as_slice()[0];
            assert_eq!(
                (t.life, t.decision, t.client_knowledge, t.delivery),
                (
                    TransactionLife::Ended,
                    Decision::Committed,
                    ClientKnowledge::KnownCommitted,
                    Delivery::Delivered
                )
            );
            assert!(t.receipt.is_some());

            // Commit continuation: Attempted, DurableConfirmed, Published, Finish Committed.
            let s = run_raw(&[1264, 6, 400, 1504, 1508, 1509, 1510, 1518]);
            assert_eq!(
                classes(&s)[3..],
                [
                    EdgeClass::Admitted,
                    EdgeClass::NoContextIssue,
                    EdgeClass::NoContextIssue,
                    EdgeClass::NoContextIssue,
                    EdgeClass::Continuation
                ]
            );
            assert_eq!(
                s[4].1.transactions.as_slice()[0].decision,
                Decision::Unknown
            );
            let t = s[7].1.transactions.as_slice()[0];
            assert_eq!(
                (t.life, t.decision, t.client_knowledge, t.delivery),
                (
                    TransactionLife::Ended,
                    Decision::Committed,
                    ClientKnowledge::KnownCommitted,
                    Delivery::Delivered
                )
            );
            assert!(s[7].1.actors.as_slice()[0].transaction.is_none());

            // Bind: N1 SuccessA installs A61; Count2 is a BindCount value error.
            let s = run_raw(&[3, 48, 58]);
            assert_eq!(classes(&s), [EdgeClass::Admitted; 3]);
            assert_eq!(
                s[1].1.bindings.as_slice()[0].value,
                super::schema::BindingValue::A61
            );
            assert_eq!(
                s[2].0.response.expect("response").code,
                ReturnCode::BindCount
            );

            // CloseStatement then slot reuse retires the closed statement.
            let s = run_raw(&[0, 400, 753, 1168, 0]);
            assert_eq!(
                s[3].1.issued_statements.as_slice()[0].life,
                StatementLife::CurrentClosed
            );
            let old = s[4].1.issued_statements.as_slice()[0];
            assert_eq!(
                (old.life, old.retirement),
                (StatementLife::Retired, super::schema::Retirement::SlotReuse)
            );

            // Cached EOF and idempotent close preserve the manager and consume nothing.
            let s = run_raw(&[0, 400, 753, 753]);
            let (edge, _) = &s[3];
            assert_eq!((edge.class, edge.consumed_parts), (EdgeClass::Admitted, 0));
            assert_eq!(edge.response.expect("response").code, ReturnCode::Eof);
            let s = run_raw(&[0, 400, 1072, 1072]);
            assert_eq!(
                (s[3].0.class, s[3].0.consumed_parts),
                (EdgeClass::Admitted, 0)
            );

            // Execute Conflict aborts the explicit RW transaction.
            let s = run_raw(&[1264, 6, 407]);
            assert_eq!(
                s[2].0.response.expect("response").code,
                ReturnCode::Conflict
            );
            let t = s[2].1.transactions.as_slice()[0];
            assert_eq!(
                (t.life, t.decision),
                (TransactionLife::Ended, Decision::Aborted)
            );

            // Fetch whole-abort Incomplete on an explicit RW transaction fences the engine.
            let s = run_raw(&[1264, 0, 400, 761]);
            assert_eq!(
                s[3].0.response.expect("response").code,
                ReturnCode::RecoveryRequired
            );
            assert_eq!(
                s[3].1.engines.as_slice()[0].admission,
                EngineAdmission::Fenced
            );
            assert_eq!(
                s[3].1.transactions.as_slice()[0].life,
                TransactionLife::RecoveryPending
            );

            // A recovery set without its first obligation is excluded.
            let s = run_raw(&[6, 409, 1310, 1314, 1529]);
            assert_eq!(s[4].0.class, EdgeClass::Excluded(Exclusion::RecoverySet));

            // Finish Unknown and PublicationPending fence with an uncertain response.
            let s = run_raw(&[1264, 6, 400, 1504, 1508, 1521]);
            let r = s[5].0.response.expect("response");
            assert_eq!(
                (s[5].0.class, r.code, r.engine_cause),
                (
                    EdgeClass::Continuation,
                    ReturnCode::ResultUnknown,
                    EngineCause::CommitUnknown
                )
            );
            let s = run_raw(&[1264, 6, 400, 1504, 1508, 1509, 1522]);
            let r = s[6].0.response.expect("response");
            assert_eq!(
                (r.code, r.engine_cause),
                (
                    ReturnCode::ResultUnknown,
                    EngineCause::CommittedPublicationPending
                )
            );

            // A peer at DurableConfirmed during a fence drains its obligation when Published.
            let s = run_raw(&[1267, 30, 576, 1506, 1513, 1514, 1264, 1303, 1515, 1523]);
            assert_eq!(s[8].0.class, EdgeClass::NoContextIssue);
            let drained = s[8]
                .1
                .obligations
                .as_slice()
                .iter()
                .find(|o| o.transaction.0 == 1)
                .expect("peer obligation");
            assert_eq!(drained.settlement, Settlement::DrainedConfirmed);
            assert_eq!(s[9].0.class, EdgeClass::Continuation);

            // Healthy same-engine reopen after a close.
            let s = run_raw(&[1310, 1318]);
            assert_eq!(
                classes(&s),
                [EdgeClass::Admitted, EdgeClass::NoContextIssue]
            );
            assert_eq!(s[1].1.actors.as_slice()[0].session.0, 3);
        })
        .expect("worker");
    if let Err(p) = worker.join() {
        std::panic::resume_unwind(p);
    }
}
