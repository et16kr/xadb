//! Test-only pinned input loading, exact invocation selection, and first-run stack boundary.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::OnceLock;

const GROUPS: [(&str, usize); 8] = [
    ("raw1533", 1533),
    ("invalid_index", 3),
    ("invalid_fields", 123),
    ("selector_baseline", 96),
    ("selector_history", 9),
    ("selector_namespace", 12),
    ("local_errors", 115),
    ("readset_control", 60),
];

struct Fixtures {
    raw: Value,
    invalid: Value,
    selectors: Value,
}

fn pinned_json(name: &str, sha: &str) -> Value {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tests/fixtures/dev4-lifecycle-raw-selection");
    let bytes = std::fs::read(directory.join(name)).expect("read checked-in raw fixture");
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        sha,
        "fixture pin {name}"
    );
    serde_json::from_slice(&bytes).expect("pinned fixture JSON")
}

fn fixtures() -> &'static Fixtures {
    static FIXTURES: OnceLock<Fixtures> = OnceLock::new();
    FIXTURES.get_or_init(|| {
        let manifest = pinned_json(
            "manifest.json",
            "bc1de24bd9c61ac7e485bffeb3b5c2dff94bf4a0882ddc15c30d106299fe9eba",
        );
        let inventory = pinned_json(
            "selection-inventory.json",
            "7a634d18274e0db668146b10fb9bff3083ba0686e018ca6b486eb87c58380bbc",
        );
        let value = Fixtures {
            raw: pinned_json(
                "raw-descriptors.json",
                "001f1b66bd43b86fae4188c2c0e2b6ec6c360c43fe5e619f27a7b976bc60bedf",
            ),
            invalid: pinned_json(
                "raw-invalid.json",
                "9b202059826dafe7755034776662f3adeffa2d29aa3e2cd65c4cfed0079058dd",
            ),
            selectors: pinned_json(
                "selectors.json",
                "89a4daaa657489c9706906a0cfde3aa2e0d046f3c5d3852060baefcf0f897da0",
            ),
        };
        let rows: Vec<_> = array(&value.raw["cases"])
            .iter()
            .chain(array(&value.invalid["invalid_index"]))
            .chain(array(&value.invalid["invalid_fields"]))
            .chain(array(&value.selectors["cases"]))
            .collect();
        let ids: Vec<_> = rows.iter().map(|v| text(&v["id"])).collect();
        assert_eq!(ids.len(), 1951);
        assert_eq!(ids.iter().copied().collect::<BTreeSet<_>>().len(), 1951);
        assert_eq!(
            ids,
            array(&inventory["ordered_case_ids"])
                .iter()
                .map(text)
                .collect::<Vec<_>>()
        );
        for (group, count) in GROUPS {
            assert_eq!(
                rows.iter().filter(|v| text(&v["group"]) == group).count(),
                count,
                "frozen group {group}"
            );
            assert_eq!(number(&inventory["groups"][group]), count as u64);
        }
        assert_eq!(number(&manifest["logical_cases"]), 1951);
        assert_eq!(number(&inventory["logical_cases"]), 1951);
        assert_eq!(number(&manifest["planned_api_calls"]), 3484);
        for (key, count) in [
            ("action_at", 1536),
            ("index_of", 1656),
            ("resolve_selector", 292),
            ("total", 3484),
        ] {
            assert_eq!(number(&inventory["planned_api_calls"][key]), count);
        }
        value
    })
}

/// Read one complete group; expected counts refer to frozen inputs, not executions.
pub(super) fn group_cases(group: &str) -> Vec<&'static Value> {
    let input = fixtures();
    let rows = match group {
        "raw1533" => array(&input.raw["cases"]),
        "invalid_index" | "invalid_fields" => array(&input.invalid[group]),
        _ => array(&input.selectors["cases"]),
    };
    let selected: Vec<_> = rows.iter().filter(|v| text(&v["group"]) == group).collect();
    let count = GROUPS
        .iter()
        .find(|(name, _)| *name == group)
        .expect("known group")
        .1;
    assert_eq!(selected.len(), count, "frozen group length {group}");
    selected
}

/// One exact API direction; raw round-trip directions are independently resumable.
#[derive(Clone, Copy)]
pub(super) struct Invocation {
    pub(super) case: usize,
    pub(super) direction: &'static str,
}

fn environment(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(value) => Some(value),
        Err(std::env::VarError::NotPresent) => None,
        Err(error) => panic!("invalid {name}: {error}"),
    }
}

fn decimal(name: &str, input: &str) -> usize {
    assert!(
        !input.is_empty() && input.bytes().all(|b| b.is_ascii_digit()),
        "{name} must be unsigned decimal"
    );
    input
        .parse()
        .unwrap_or_else(|_| panic!("{name} is too large"))
}

/// Select exact IDs (optionally `#direction`) or a strict group-local half-open range.
/// START_DIRECTION skips only completed directions of the first selected case.
pub(super) fn select(group: &str, ids: &[String], directions: &[&'static str]) -> Vec<Invocation> {
    let id_input = environment("XADB_LIFECYCLE_RAW_CASE_IDS");
    let start_input = environment("XADB_LIFECYCLE_RAW_START");
    let end_input = environment("XADB_LIFECYCLE_RAW_END");
    let first_direction = environment("XADB_LIFECYCLE_RAW_START_DIRECTION");
    let mut output = Vec::new();
    if let Some(input) = id_input {
        assert!(
            start_input.is_none() && end_input.is_none() && first_direction.is_none(),
            "ID and range selection cannot be combined"
        );
        let mut requested = BTreeSet::new();
        for item in input.split(',') {
            assert!(
                !item.is_empty() && item.trim() == item,
                "empty or padded selected ID"
            );
            let (id, chosen) = match item.split_once('#') {
                Some((id, direction)) => (id, Some(direction)),
                None => (item, None),
            };
            let index = ids
                .iter()
                .position(|entry| entry == id)
                .unwrap_or_else(|| panic!("ID {id} is not in selected group {group}"));
            if let Some(direction) = chosen {
                assert!(
                    directions.contains(&direction),
                    "unsupported direction {direction} for {group}"
                );
            }
            for (position, direction) in directions.iter().enumerate() {
                if chosen.is_none_or(|name| name == *direction) {
                    assert!(
                        requested.insert((index, position)),
                        "duplicate selected invocation {item}"
                    );
                }
            }
        }
        for (case, position) in requested {
            output.push(Invocation {
                case,
                direction: directions[position],
            });
        }
    } else {
        let start = start_input
            .as_deref()
            .map(|v| decimal("START", v))
            .unwrap_or(0);
        let end = end_input
            .as_deref()
            .map(|v| decimal("END", v))
            .unwrap_or(ids.len());
        assert!(
            start < end && end <= ids.len(),
            "nonempty half-open range must lie in 0..{} for {group}",
            ids.len()
        );
        let first = first_direction
            .as_deref()
            .map(|value| {
                directions
                    .iter()
                    .position(|d| *d == value)
                    .unwrap_or_else(|| panic!("unsupported START_DIRECTION {value} for {group}"))
            })
            .unwrap_or(0);
        for case in start..end {
            for direction in directions
                .iter()
                .skip(if case == start { first } else { 0 })
            {
                output.push(Invocation { case, direction });
            }
        }
    }
    assert!(
        !output.is_empty(),
        "zero selected invocations are not a passing run"
    );
    let count = output.iter().map(|v| v.case).collect::<BTreeSet<_>>().len();
    println!(
        "SELECTED group={group} logical_cases={count} api_calls={} frozen_group_cases={} stack_bytes=262144",
        output.len(),
        ids.len()
    );
    output
}

/// First execution of selected calls uses a 256KiB thread; no second stack replay.
pub(super) fn run_256k(group: &'static str, body: impl FnOnce() + Send + 'static) {
    let worker = std::thread::Builder::new()
        .name(format!("lm-{group}"))
        .stack_size(256 * 1024)
        .spawn(body)
        .expect("spawn selected 256KiB worker");
    if let Err(payload) = worker.join() {
        std::panic::resume_unwind(payload);
    }
}

/// Preserve the exact failing direction and stop before any following invocation.
pub(super) fn step(group: &str, id: &str, direction: &str, body: impl FnOnce()) {
    println!("START group={group} id={id} direction={direction}");
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)) {
        Ok(()) => println!("COMPLETED group={group} id={id} direction={direction}"),
        Err(payload) => {
            println!("FAIL group={group} id={id} direction={direction} remaining=NOT_RUN");
            std::panic::resume_unwind(payload);
        }
    }
}

/// Exact fixture string, rejecting silent coercion.
pub(super) fn text(value: &Value) -> &str {
    value.as_str().expect("literal string")
}
/// Exact fixture array.
pub(super) fn array(value: &Value) -> &[Value] {
    value.as_array().expect("literal array")
}
/// Exact nonnegative integer.
pub(super) fn number(value: &Value) -> u64 {
    value.as_u64().expect("literal unsigned integer")
}
/// Checked u8 field; malformed logical values still fit this representation.
pub(super) fn byte(value: &Value) -> u8 {
    u8::try_from(number(value)).expect("literal u8")
}
/// Checked u16 field.
pub(super) fn word(value: &Value) -> u16 {
    u16::try_from(number(value)).expect("literal u16")
}
