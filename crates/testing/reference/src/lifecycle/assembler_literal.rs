//! Test-only reading of the independent full-input assembly literals.
//!
//! Derived pre-states are applied as base/put deltas through the frozen lifecycle literal
//! reader; expected inputs are read field by field. No assembler, model or codec output is
//! used to manufacture an expectation.
use super::input_assembler::AssemblyError;
use super::input_schema::{
    BindValue, Environment, InputObligation, InputOwner, RawIdentity, ResolvedInput,
};
use super::literal::{Literals, SOURCE};
use super::raw_action::{PrepareChoice, RawAction, RawActionError};
use super::schema::{Autocommit, Records, Shape};
use super::target_selector::{SelectorError, TargetSelector};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::fmt::Debug;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Frozen groups: (name, file, case count, SHA-256 of the checked-in file).
pub(super) const GROUPS: [(&str, &str, usize, &str); 10] = [
    (
        "initial",
        "normal/initial.json",
        1533,
        "19e598b6f05580e043ae45fa75abfb66ed177674fade8ca04fdc61f7988dd126",
    ),
    (
        "original",
        "normal/original-links.json",
        27,
        "0fb2dabb3789eaf2554bf6c0e95a29f5bb69b8357a0d7255742c57fc468d38ea",
    ),
    (
        "bind",
        "normal/rich-bind.json",
        22,
        "ac3d034baa50cea0d77883cd8a550e8bbc15018d3b30936bd8390ef9d34555da",
    ),
    (
        "boundary",
        "boundaries/cases.json",
        76,
        "b3e17c5957ee204cef8f986d810d9141f37a3d4220a04c30268af283a49782b5",
    ),
    (
        "rich_normal",
        "rich/rich-normal-2.json",
        97,
        "75351fdad1b745339ad9b0e8fc3c461d5ef4cc8a1efacd9428403da6b68f1502",
    ),
    (
        "rich_continuation",
        "rich/rich-continuation-3.json",
        77,
        "a3eec8642e6c927fef37ba0bc9c29085b495c7e6fb0da28223ae3198b81653b3",
    ),
    (
        "rich_rest",
        "rich/rich-normal-rest-4.json",
        130,
        "3b7be4f6e43bbd3181a18e7e0ea971c497299d99104570f78ee127ca79e32375",
    ),
    (
        "supplement",
        "supplement/cases.json",
        17,
        "429ffe0a553c412531fba3a7c80bc00dd43bdf1c31012ed8eb931f65bc170c1d",
    ),
    (
        "supplement2",
        "supplement/cases-2.json",
        9,
        "dac1ef3d4e6518766955c48d01eba7d229eaf52c50e6bdf3d2d16d179f26f5e7",
    ),
    (
        "supplement3",
        "supplement/cases-3.json",
        6,
        "f2ad0ed9c152ecfa97ebce6391e4a965247df576cffbff2acfe61db445bb6d16",
    ),
];
const LITERALS_SHA: &str = "ec4a7940a457882c155eb84e2d9e5602eaf880563bab4f6f04d0b643bcec42d2";
/// The checked-in selection inventory must name exactly the groups pinned here.
const INVENTORY: (&str, &str) = (
    "selection-inventory.json",
    "3f43e458a8a7cd5ea30ca702aa4172248c41795c9926a53c63c51c7206475a9b",
);
/// Reviewed corrections of unreviewed rich candidates; originals stay byte-identical.
const CORRECTION: (&str, &str) = (
    "rich/correction.json",
    "f88b6517d9ac1a007da30e2c94ec1f86d631bd04e37e548eedc18653a00e2640",
);

/// Expected result of one assembly call.
pub(super) enum Expected {
    Input(Box<ResolvedInput>),
    Error(AssemblyError),
}

/// One frozen case with its pre-state literal name and raw descriptor.
pub(super) struct Case {
    pub(super) id: String,
    pub(super) pre_state: String,
    pub(super) action: RawAction,
    pub(super) expected: Expected,
}

pub(super) struct Fixtures {
    pub(super) literals: Literals,
    pub(super) groups: Vec<(&'static str, Vec<Case>)>,
    pub(super) original: Value,
}

fn directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tests/fixtures/dev4-lifecycle-assembly")
}

fn pinned(name: &str, sha: &str) -> Value {
    let bytes = std::fs::read(directory().join(name)).expect("read assembly fixture");
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        sha,
        "fixture pin {name}"
    );
    serde_json::from_slice(&bytes).expect("assembly fixture JSON")
}

/// Load every frozen group and register derived pre-states as literal deltas.
pub(super) fn fixtures() -> &'static Fixtures {
    static FIXTURES: OnceLock<Fixtures> = OnceLock::new();
    FIXTURES.get_or_init(|| {
        assert_eq!(
            format!("{:x}", Sha256::digest(SOURCE.as_bytes())),
            LITERALS_SHA,
            "lifecycle literal pin"
        );
        let mut source: Value = serde_json::from_str(SOURCE).expect("literal JSON");
        let mut groups = Vec::new();
        let mut original = Value::Null;
        let mut seen = std::collections::BTreeSet::new();
        let inventory = pinned(INVENTORY.0, INVENTORY.1);
        let listed = inventory["groups"].as_array().expect("inventory groups");
        assert_eq!(listed.len(), GROUPS.len());
        for (row, (name, file, count, sha)) in listed.iter().zip(GROUPS) {
            assert_eq!(text(&row["group"]), name);
            assert_eq!(text(&row["path"]), file);
            assert_eq!(row["cases"].as_u64(), Some(count as u64));
            assert_eq!(text(&row["sha256"]), sha);
        }
        let total: usize = GROUPS.iter().map(|g| g.2).sum();
        assert_eq!(inventory["logical_cases"].as_u64(), Some(total as u64));
        let correction = pinned(CORRECTION.0, CORRECTION.1);
        assert_eq!(text(&inventory["corrections"]["sha256"]), CORRECTION.1);
        let mut applied = 0;
        for (name, file, count, sha) in GROUPS {
            let mut value = pinned(file, sha);
            if text(&correction["source_file"]) == file {
                assert_eq!(text(&correction["source_sha256"]), sha);
                applied += correct(&mut value, &correction);
            }
            if let Some(prestates) = value.get("prestates") {
                let states = source["states"].as_object_mut().expect("states map");
                for prestate in prestates.as_array().expect("prestates") {
                    let id = text(&prestate["id"]).to_owned();
                    let base = state_name(text(&prestate["base"]));
                    let mut delta = Map::new();
                    delta.insert("base".into(), Value::String(base.to_owned()));
                    delta.insert("put".into(), prestate["put"].clone());
                    assert!(
                        states.insert(id.clone(), Value::Object(delta)).is_none(),
                        "duplicate pre-state {id}"
                    );
                }
            }
            let cases: Vec<_> = value["cases"]
                .as_array()
                .expect("cases")
                .iter()
                .map(read_case)
                .collect();
            assert_eq!(cases.len(), count, "frozen group length {name}");
            for case in &cases {
                assert!(seen.insert(case.id.clone()), "duplicate case {}", case.id);
            }
            if name == "original" {
                original = value;
            }
            groups.push((name, cases));
        }
        assert_eq!(seen.len(), 1994);
        assert_eq!(
            applied,
            correction["corrections"]
                .as_array()
                .expect("corrections")
                .len()
        );
        let literals = Literals::from_json(&source.to_string()).expect("derived literals");
        Fixtures {
            literals,
            groups,
            original,
        }
    })
}

/// Replace listed expectations after checking the exact original logical value.
pub(super) fn correct(group: &mut Value, correction: &Value) -> usize {
    let mut applied = 0;
    for item in correction["corrections"].as_array().expect("corrections") {
        let id = text(&item["id"]);
        let mut matching = group["cases"]
            .as_array_mut()
            .expect("cases")
            .iter_mut()
            .filter(|c| c["id"] == item["id"]);
        let case = matching
            .next()
            .unwrap_or_else(|| panic!("correction target {id}"));
        let original = serde_json::to_string(&case["expected_input"]).expect("canonical JSON");
        assert_eq!(
            format!("{:x}", Sha256::digest(original.as_bytes())),
            text(&item["original_expected_logical_sha256"]),
            "correction original {id}"
        );
        case["expected_input"] = item["corrected_expected_input"].clone();
        assert!(
            matching.next().is_none(),
            "duplicate correction target {id}"
        );
        applied += 1;
    }
    applied
}

/// Literal state name of an `original:` or derived pre-state reference.
pub(super) fn state_name(reference: &str) -> &str {
    reference.strip_prefix("original:").unwrap_or(reference)
}

fn read_case(value: &Value) -> Case {
    let expected = match (value.get("expected_input"), value.get("expected_error")) {
        (Some(input), None) => Expected::Input(Box::new(read_input(input))),
        (None, Some(error)) => Expected::Error(read_error(error)),
        _ => panic!("case needs exactly one expectation: {}", value["id"]),
    };
    Case {
        id: text(&value["id"]).to_owned(),
        pre_state: text(&value["pre_state"]).to_owned(),
        action: read_action(&value["raw_action"]),
        expected,
    }
}

fn read_action(value: &Value) -> RawAction {
    exact(
        value,
        &[
            "api",
            "actor",
            "selector",
            "prepare",
            "autocommit",
            "outcome",
        ],
    );
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
            shape: named::<Shape>(&v["shape"]),
            parameter_count: byte(&v["parameter_count"]),
        }
    });
    RawAction {
        api: named(&value["api"]),
        actor: byte(&value["actor"]),
        selector,
        prepare,
        autocommit: (!value["autocommit"].is_null())
            .then(|| named::<Autocommit>(&value["autocommit"])),
        outcome: byte(&value["outcome"]),
    }
}

fn read_error(value: &Value) -> AssemblyError {
    match text(&value["kind"]) {
        "InvalidAction" => {
            exact(value, &["kind", "inner"]);
            match text(&value["inner"]) {
                "InvalidFields" => AssemblyError::InvalidAction(RawActionError::InvalidFields),
                other => panic!("unknown raw error {other}"),
            }
        }
        "Selector" => {
            exact(value, &["kind", "inner"]);
            AssemblyError::Selector(debug_named::<SelectorError>(
                text(&value["inner"]),
                &[
                    SelectorError::InvalidActor,
                    SelectorError::InvalidSelector,
                    SelectorError::MissingActor,
                    SelectorError::DuplicateActor,
                    SelectorError::InvalidCallerScope,
                    SelectorError::MissingSlot,
                    SelectorError::DuplicateSlot,
                    SelectorError::InvalidGeneration,
                    SelectorError::InvalidIssuedRecord,
                    SelectorError::DuplicateOrdinal,
                    SelectorError::DuplicateNonce,
                    SelectorError::InvalidNextNonce,
                ],
            ))
        }
        kind => {
            exact(value, &["kind"]);
            match kind {
                "InvalidState" => AssemblyError::InvalidState,
                "ProfileLimit" => AssemblyError::ProfileLimit,
                "Capacity" => AssemblyError::Capacity,
                other => panic!("unknown assembly error {other}"),
            }
        }
    }
}

fn read_input(value: &Value) -> ResolvedInput {
    exact(
        value,
        &[
            "schema_version",
            "api",
            "api_tag",
            "actor",
            "outcome_tag",
            "prepare_slot",
            "prepare_shape",
            "parameter_count",
            "set_autocommit",
            "target",
            "context",
            "original_deadline",
            "supplied_cancel",
            "deadline_reached",
            "bind_values",
            "environment",
        ],
    );
    assert_eq!(byte(&value["schema_version"]), 1);
    let api = named(&value["api"]);
    assert_eq!(byte(&value["api_tag"]), api as u8, "named/numeric API");
    let mut bind_values = Records::default();
    for row in value["bind_values"].as_array().expect("bind values") {
        exact(row, &["tag", "byte_length", "bytes"]);
        let bytes = row["bytes"].as_array().expect("bind bytes");
        let length = word(&row["byte_length"]);
        assert!(bytes.len() == usize::from(length) && bytes.len() <= 8);
        let mut out = BindValue {
            tag: byte(&row["tag"]),
            byte_length: length,
            bytes: [0; 8],
        };
        for (slot, b) in out.bytes.iter_mut().zip(bytes) {
            *slot = byte(b);
        }
        bind_values.try_push(out).expect("bind capacity");
    }
    ResolvedInput {
        api,
        actor: byte(&value["actor"]),
        outcome: byte(&value["outcome_tag"]),
        prepare_slot: byte(&value["prepare_slot"]),
        prepare_shape: byte(&value["prepare_shape"]),
        prepare_n: byte(&value["parameter_count"]),
        set_autocommit: byte(&value["set_autocommit"]),
        target: optional_identity(&value["target"]),
        context: optional_identity(&value["context"]),
        original_deadline: named(&value["original_deadline"]),
        cancel: boolean(&value["supplied_cancel"]),
        deadline_reached: boolean(&value["deadline_reached"]),
        bind_values,
        environment: (!value["environment"].is_null())
            .then(|| read_environment(&value["environment"])),
    }
}

fn read_environment(value: &Value) -> Environment {
    exact(
        value,
        &[
            "envelope_schema",
            "fact",
            "kind",
            "work_kind",
            "work_class",
            "cleanup",
            "commit_class",
            "reserved0",
            "transaction",
            "receipt",
            "anchor",
            "fresh_engine",
            "obligations",
            "owners",
        ],
    );
    assert_eq!(byte(&value["envelope_schema"]), 1);
    assert_eq!(byte(&value["reserved0"]), 0);
    let mut out = Environment {
        kind: named(&value["kind"]),
        fact: identity(&value["fact"]),
        work_kind: match text(&value["work_kind"]) {
            "None" => 0,
            "Execute" => 1,
            "Fetch" => 2,
            "Resume" => 3,
            other => panic!("unknown work kind {other}"),
        },
        work_class: byte(&value["work_class"]),
        cleanup: byte(&value["cleanup"]),
        commit_class: byte(&value["commit_class"]),
        transaction: optional_identity(&value["transaction"]),
        receipt: optional_identity(&value["receipt"]),
        anchor: optional_identity(&value["anchor"]),
        fresh_engine: optional_identity(&value["fresh_engine"]),
        obligations: Records::default(),
        owners: Records::default(),
    };
    for row in value["obligations"].as_array().expect("obligations") {
        exact(
            row,
            &[
                "obligation",
                "known_decision",
                "after_ownership_summary",
                "associated_owner_count",
            ],
        );
        out.obligations
            .try_push(InputObligation {
                obligation: identity(&row["obligation"]),
                known_decision: named(&row["known_decision"]),
                after_ownership_summary: named(&row["after_ownership_summary"]),
                associated_owner_count: word(&row["associated_owner_count"]),
            })
            .expect("obligation capacity");
    }
    for row in value["owners"].as_array().expect("owners") {
        exact(
            row,
            &[
                "resource",
                "resource_kind",
                "after_disposition",
                "obligation",
                "before_holder",
                "after_holder",
            ],
        );
        out.owners
            .try_push(InputOwner {
                resource: identity(&row["resource"]),
                resource_kind: named(&row["resource_kind"]),
                after_disposition: named(&row["after_disposition"]),
                obligation: optional_identity(&row["obligation"]),
                before_holder: optional_identity(&row["before_holder"]),
                after_holder: optional_identity(&row["after_holder"]),
            })
            .expect("owner capacity");
    }
    out
}

/// Identity16 from the literal seven fields: kind, actor, slot, E, S, nonce, generation.
fn identity(value: &Value) -> RawIdentity {
    let parts = value.as_array().expect("identity array");
    assert_eq!(parts.len(), 7, "identity has seven fields");
    let mut bytes = [0; 16];
    for (offset, part) in parts[..3].iter().enumerate() {
        bytes[offset] = byte(part);
    }
    for (offset, part) in [4, 6, 8, 10].into_iter().zip(&parts[3..]) {
        bytes[offset..offset + 2].copy_from_slice(&word(part).to_le_bytes());
    }
    RawIdentity(bytes)
}
fn optional_identity(value: &Value) -> Option<RawIdentity> {
    (!value.is_null()).then(|| identity(value))
}

/// A frozen vocabulary value by its contract name.
fn named<T: TryFrom<u8> + Debug>(value: &Value) -> T {
    let name = text(value);
    (0..=u8::MAX)
        .filter_map(|tag| T::try_from(tag).ok())
        .find(|candidate| format!("{candidate:?}") == name)
        .unwrap_or_else(|| panic!("unknown vocabulary value {name}"))
}
fn debug_named<T: Copy + Debug>(name: &str, values: &[T]) -> T {
    *values
        .iter()
        .find(|v| format!("{v:?}") == name)
        .unwrap_or_else(|| panic!("unknown value {name}"))
}

fn exact(value: &Value, names: &[&str]) {
    let fields = value.as_object().expect("object");
    assert!(
        fields.len() == names.len() && names.iter().all(|n| fields.contains_key(*n)),
        "expected fields {names:?}, got {:?}",
        fields.keys().collect::<Vec<_>>()
    );
}
pub(super) fn text(value: &Value) -> &str {
    value.as_str().expect("literal string")
}
fn number(value: &Value) -> u64 {
    value.as_u64().expect("literal unsigned integer")
}
fn byte(value: &Value) -> u8 {
    u8::try_from(number(value)).expect("literal u8")
}
fn word(value: &Value) -> u16 {
    u16::try_from(number(value)).expect("literal u16")
}
fn boolean(value: &Value) -> bool {
    value.as_bool().expect("literal bool")
}
