//! Selected checks for the new literal reader and its independent byte inputs.

use super::codec::{decode_input, decode_state, encode_input, encode_state};
use super::input_schema::ResolvedInput;
use super::literal::{Literals, SOURCE};
use super::schema::{ContextData, State, WireIdentity};
use super::workspace::{MAX_INPUT_BYTES, MAX_STATE_BYTES, Workspace};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

const SOURCE_SHA: &str = "ec4a7940a457882c155eb84e2d9e5602eaf880563bab4f6f04d0b643bcec42d2";
const MANIFEST_SHA: &str = "e58a94580d9f1edcababa0a36458582c70b798a4346a6d36f309391deec25cad";
const SELECTION_SHA: &str = "8dc9e4325150f53663ed4d5e11426da2e7270675b43e59dc3220a598b0d42911";
const STATE_NAMES: [&str; 29] = [
    "initial",
    "sel.prepared",
    "sel.open",
    "sel.eof",
    "sel.reexecuted",
    "sel.stale_rejected",
    "sel.current_rows",
    "sel.rows",
    "sel.parked",
    "sel.signalled",
    "sel.timeout",
    "sel.cancelled",
    "sel.entry_cancelled",
    "sel.buffer_small",
    "sel.closed",
    "rw.begun",
    "rw.prepared",
    "rw.written",
    "rw.select_prepared",
    "rw.select_open",
    "rw.select_closed",
    "rw.aborted",
    "rw.recovery_pending",
    "off.set",
    "off.prepared",
    "off.written",
    "off.select_prepared",
    "off.select_open",
    "off.select_closed",
];

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tests/fixtures/dev4-lifecycle-model")
}

fn pinned_json(name: &str, expected_sha: &str) -> Value {
    let bytes = std::fs::read(fixture_dir().join(name)).unwrap();
    assert_eq!(hash(&bytes), expected_sha, "source pin for {name}");
    serde_json::from_slice(&bytes).unwrap()
}

fn literals() -> Literals {
    assert_eq!(
        hash(SOURCE.as_bytes()),
        SOURCE_SHA,
        "original logical source pin"
    );
    Literals::new().unwrap()
}

fn vector_bytes(manifest: &Value, id: &str, expected_length: usize) -> Vec<u8> {
    let matches: Vec<_> = manifest["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["id"].as_str() == Some(id))
        .collect();
    assert_eq!(matches.len(), 1, "unique vector {id}");
    let entry = matches[0];
    let relative = entry["binary"].as_str().unwrap();
    assert_eq!(relative, format!("vectors/{id}.bin"));
    let bytes = std::fs::read(fixture_dir().join(relative)).unwrap();
    assert_eq!(bytes.len(), expected_length, "literal length {id}");
    assert_eq!(
        entry["byte_length"].as_u64().unwrap() as usize,
        expected_length
    );
    assert_eq!(
        hash(&bytes),
        entry["byte_sha256"].as_str().unwrap(),
        "binary pin {id}"
    );
    bytes
}

fn assert_state_fields(actual: &State, expected: &State, id: &str) {
    assert_eq!(
        actual.current_engine, expected.current_engine,
        "{id}/current_engine"
    );
    assert_eq!(actual.next_nonce, expected.next_nonce, "{id}/next_nonce");
    // Records equality intentionally compares only the initialized prefix. All
    // fields of each record participate; native padding is not a logical field.
    macro_rules! compare {
        ($($name:ident),+ $(,)?) => { $(
            assert_eq!(actual.$name.as_slice(), expected.$name.as_slice(), "{id}/{}", stringify!($name));
        )+ };
    }
    compare!(
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
}

#[test]
fn literal_state_inventory() {
    let fixture = literals();
    let mut actual_names = fixture.state_names().unwrap();
    let mut expected_names = STATE_NAMES.to_vec();
    actual_names.sort_unstable();
    expected_names.sort_unstable();
    assert_eq!(actual_names, expected_names);
    let mut workspace = Workspace::try_new().unwrap();
    let mut count = 0;
    // Reuse one heap State while reading all29; no array of native State values.
    for name in STATE_NAMES {
        fixture.state(name, workspace.state_mut()).unwrap();
        let state = workspace.state();
        assert_eq!(state.actors.len(), 2, "{name}/actors");
        assert_eq!(state.prepared_slots.len(), 4, "{name}/prepared_slots");
        for context in state.contexts.as_slice() {
            if let ContextData::ParkedFetch(parked) = context.data {
                assert!(parked.statement.is_some() && parked.result.is_some());
            }
        }
        count += 1;
    }
    assert_eq!(count, 29);
    eprintln!("literal_state_inventory: 29 literal states constructed; model/edge checks NOT_RUN");
}

#[test]
fn literal_state_byte_vectors() {
    // Validate the selector before any fixture/codec calls. A resumed run keeps
    // its original case indices and never reports the omitted prefix as tested.
    let start = match std::env::var("XADB_LIFECYCLE_STATE_VECTOR_START") {
        Err(std::env::VarError::NotPresent) => 0,
        Ok(value) => match value.as_str() {
            "0" => 0,
            "1" => 1,
            "2" => 2,
            "3" => 3,
            _ => panic!("XADB_LIFECYCLE_STATE_VECTOR_START must be exactly 0, 1, 2, or 3"),
        },
        Err(std::env::VarError::NotUnicode(_)) => {
            panic!("XADB_LIFECYCLE_STATE_VECTOR_START must be Unicode decimal 0..3")
        }
    };
    let fixture = literals();
    let manifest = pinned_json("independent-vector-manifest.json", MANIFEST_SHA);
    let selection = pinned_json("vector-selection.json", SELECTION_SHA);
    assert_eq!(manifest["state_vector_count"], 4);
    assert_eq!(selection["state_vectors"].as_array().unwrap().len(), 4);
    let identity = WireIdentity {
        layout: [0x00; 32],
        profile: [0x11; 32],
        action: [0x22; 32],
        exclusion: [0x33; 32],
    };
    let mut expected = Workspace::try_new().unwrap();
    let mut actual = Workspace::try_new().unwrap();
    let mut output = vec![0; MAX_STATE_BYTES];
    let cases = [
        ("state-initial", "initial", 444),
        ("state-sel-parked", "sel.parked", 946),
        ("state-sel-stale-rejected", "sel.stale_rejected", 1313),
        ("state-rw-recovery-pending", "rw.recovery_pending", 1058),
    ];
    let selected = cases.len() - start;
    let mut completed = 0;
    eprintln!(
        "literal_state_byte_vectors: inventory=4 start={start} selected={selected} omitted_prefix={start}"
    );
    for (index, (id, name, length)) in cases.into_iter().enumerate().skip(start) {
        assert_eq!(selection["state_vectors"][index]["id"], id);
        assert_eq!(selection["state_vectors"][index]["literal_state"], name);
        let bytes = vector_bytes(&manifest, id, length);
        fixture.state(name, expected.state_mut()).unwrap();
        let written = encode_state(expected.state(), &identity, &mut output).unwrap();
        assert_eq!(&output[..written], bytes, "independent state encode {id}");
        decode_state(&bytes, &identity, actual.state_mut()).unwrap();
        assert_state_fields(actual.state(), expected.state(), id);
        completed += 1;
        eprintln!(
            "literal_state_byte_vectors: COMPLETED index={index} id={id} encode=1 decode=1 full_field_compare=1"
        );
    }
    assert_eq!(completed, selected);
    eprintln!(
        "literal_state_byte_vectors: selected={selected} completed={completed} encode={completed} decode={completed}; omitted prefix NOT_RUN in this invocation"
    );
}

#[test]
fn literal_input_byte_vectors() {
    let fixture = literals();
    let manifest = pinned_json("independent-vector-manifest.json", MANIFEST_SHA);
    let selection = pinned_json("vector-selection.json", SELECTION_SHA);
    assert_eq!(manifest["input_vector_count"], 7);
    assert_eq!(selection["input_vectors"].as_array().unwrap().len(), 7);
    let cases = [
        ("input-select-prepare", "sel.prepare", 32),
        ("input-select-execute", "sel.execute", 214),
        ("input-atomic-terminal-cancel", "sel.terminal_cancel", 214),
        ("input-resume-pending-deadline", "sel.resume_timeout", 198),
        ("input-rollback-incomplete", "rw.rollback_incomplete", 371),
        ("input-stale-result", "sel.stale_rows", 81),
        ("input-malformed-child-u9", "sel.stale_rows", 81),
    ];
    let mut actual = ResolvedInput::default();
    let mut output = vec![0; MAX_INPUT_BYTES];
    for (index, (id, name, length)) in cases.into_iter().enumerate() {
        let entry = &selection["input_vectors"][index];
        assert_eq!(entry["id"], id);
        assert_eq!(entry["literal_input"], name);
        let bytes = vector_bytes(&manifest, id, length);
        let expected = fixture.input(name, entry.get("literal_put")).unwrap();
        let written = encode_input(&expected, &mut output).unwrap();
        assert_eq!(&output[..written], bytes, "independent input encode {id}");
        decode_input(&bytes, &mut actual).unwrap();
        assert_eq!(actual, expected, "independent input decode {id}");
    }
    eprintln!(
        "literal_input_byte_vectors: 7 input vectors, 7 encode and 7 decode comparisons; semantic execution NOT_RUN"
    );
}

#[test]
fn literal_envelope_cycles_are_rejected() {
    // These private reader controls change in-memory text only. The accepted
    // logical source and its independently packed byte vectors remain intact.
    let mut source: Value = serde_json::from_str(SOURCE).unwrap();
    source["inputs"]["cycle-control"] = serde_json::json!({
        "base": "sel.execute",
        "put": {"/environment": {"envelope_ref": "cycle.a"}}
    });
    source["envelopes"]["cycle.a"] = serde_json::json!({"envelope_ref": "cycle.a"});
    let fixture = Literals::from_json(&source.to_string()).unwrap();
    let error = fixture.input("cycle-control", None).unwrap_err();
    assert!(
        error.contains("cyclic literal reference envelopes/cycle.a"),
        "{error}"
    );

    source["envelopes"]["cycle.a"] = serde_json::json!({"envelope_ref": "cycle.b"});
    source["envelopes"]["cycle.b"] = serde_json::json!({"envelope_ref": "cycle.a"});
    let fixture = Literals::from_json(&source.to_string()).unwrap();
    let error = fixture.input("cycle-control", None).unwrap_err();
    assert!(
        error.contains("cyclic literal reference envelopes/cycle.a"),
        "{error}"
    );

    // A base hop must not reset the enclosing envelope traversal stack.
    source["envelopes"]["cycle.a"] = serde_json::json!({"base": "cycle.b"});
    let fixture = Literals::from_json(&source.to_string()).unwrap();
    let error = fixture.input("cycle-control", None).unwrap_err();
    assert!(
        error.contains("cyclic literal reference envelopes/cycle.a"),
        "{error}"
    );

    source["envelopes"]["cycle.b"] = serde_json::json!({"envelope_ref": "sel.execute"});
    let fixture = Literals::from_json(&source.to_string()).unwrap();
    let actual = fixture.input("cycle-control", None).unwrap();
    let expected = literals().input("sel.execute", None).unwrap();
    assert_eq!(
        actual, expected,
        "acyclic envelope/base chain preserves every input field"
    );
    eprintln!(
        "literal_envelope_cycles_are_rejected: 3 rejected cycles and 1 acyclic control; no model execution"
    );
}
