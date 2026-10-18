//! Fixed bounded mutation generation for four durable native wire codecs.
//! Literal cases have independently specified acceptance; arbitrary mutations only
//! compare framing paths sharing JSON/validation. This is neither coverage-guided
//! fuzzing nor engine/durability/PF evidence. The fault transport's separate 4096
//! message/128 KiB session limits are not imposed by these generic frame codecs.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fmt::Debug,
    fs::{self, OpenOptions},
    io::{Cursor, Write},
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    path::PathBuf,
    time::{Duration, Instant},
};
use xadbtest_protocol::{
    durable, fault,
    frame::{self, Decoder, Envelope, MAX_BODY_BYTES, MAX_FRAME_BYTES},
};

const GENERATOR: &str = "durable-native-four-codecs-mutation-v1";
const TARGET: &str = "durable_and_fault_codecs_fuzz";
const MIN_SECONDS: u64 = 1800;
const INPUT_CAP: usize = 2 * MAX_FRAME_BYTES + 4;
const SEEDS: usize = 256;
const ROUND_CAP: u64 = 1_000_000_000;
const OPERATORS: [&str; 17] = [
    "valid_identity",
    "byte_flip",
    "byte_insert",
    "byte_delete",
    "length",
    "truncate",
    "concatenate",
    "unknown_field",
    "duplicate_field",
    "unknown_tag",
    "identity_limit",
    "invalid_utf8",
    "nested_unknown",
    "nested_duplicate",
    "trailing_json",
    "partial_suffix",
    "invalid_suffix",
];
const LANES: [Lane; 4] = [
    Lane::Request,
    Lane::Response,
    Lane::FaultObservation,
    Lane::FaultRelease,
];
const METRIC_IDS: [&str; 21] = [
    "dev2.wal.encode",
    "dev2.wal.append",
    "dev2.wal.direct_sync",
    "dev2.wal.rotate",
    "dev2.wal.epoch_install",
    "dev2.recovery.inspect",
    "dev2.recovery.validate",
    "dev2.recovery.apply",
    "dev2.recovery.stabilize",
    "dev2.recovery.direct_sync",
    "dev2.checkpoint.build",
    "dev2.checkpoint.write",
    "dev2.checkpoint.direct_sync",
    "dev2.checkpoint.select",
    "dev2.backup.copy",
    "dev2.backup.file",
    "dev2.namespace.wait",
    "dev2.namespace.hold",
    "dev2.retention.inspect",
    "dev2.retention.collect",
    "dev2.namespace.direct_sync",
];
const BOUNDARIES: [&str; 13] = [
    "create_before",
    "create_after",
    "write_before",
    "write_partial",
    "write_after",
    "sync_before",
    "sync_after",
    "rename_before",
    "rename_after",
    "directory_sync_before",
    "directory_sync_after",
    "unlink_before",
    "unlink_after",
];
const TARGETS: [&str; 6] = [
    "any",
    "wal_commit",
    "recovery_old_file",
    "current",
    "writer",
    "retirement",
];
// Literal closed catalog, independent of Cut::validate.
fn allowed(target: &str, boundary: &str) -> bool {
    match target {
        "any" => true,
        "wal_commit" => matches!(
            boundary,
            "write_before" | "write_partial" | "write_after" | "sync_before" | "sync_after"
        ),
        "recovery_old_file" => matches!(boundary, "sync_before" | "sync_after"),
        "current" | "writer" => matches!(
            boundary,
            "write_before"
                | "write_partial"
                | "write_after"
                | "sync_before"
                | "sync_after"
                | "rename_before"
                | "rename_after"
                | "directory_sync_before"
                | "directory_sync_after"
        ),
        "retirement" => matches!(
            boundary,
            "unlink_before" | "unlink_after" | "directory_sync_before" | "directory_sync_after"
        ),
        _ => false,
    }
}
#[derive(Clone, Copy, Debug)]
enum Lane {
    Request,
    Response,
    FaultObservation,
    FaultRelease,
}
impl Lane {
    fn index(self) -> usize {
        match self {
            Self::Request => 0,
            Self::Response => 1,
            Self::FaultObservation => 2,
            Self::FaultRelease => 3,
        }
    }
    fn name(self) -> &'static str {
        [
            "durable_request",
            "durable_response",
            "fault_observation",
            "fault_release",
        ][self.index()]
    }
    fn wrap(self, body: Value) -> Value {
        let identity = vec![1u8; 16];
        match self {
            Self::Request => {
                json!({"version":1,"run":identity,"request":1,"actor":0,"command":{"gate":null,"action":body}})
            }
            Self::Response => {
                json!({"version":1,"run":identity,"request":1,"actor":0,"passed_gate":null,"observation":body})
            }
            Self::FaultObservation => {
                json!({"version":1,"run":identity,"token":vec![2u8;16],"event":body})
            }
            Self::FaultRelease => {
                json!({"version":1,"run":identity,"token":vec![2u8;16],"decision":body})
            }
        }
    }
}
struct Random(u64);
impl Random {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }
    fn index(&mut self, end: usize) -> usize {
        (self.next() % end as u64) as usize
    }
}
#[derive(Clone)]
struct Case {
    label: String,
    family: &'static str,
    bytes: Vec<u8>,
    expected: Option<(bool, usize)>,
}
fn raw(body: &[u8]) -> Vec<u8> {
    let mut bytes = (body.len() as u32).to_be_bytes().to_vec();
    bytes.extend_from_slice(body);
    bytes
}
fn literal(label: impl Into<String>, family: &'static str, value: Value, accept: bool) -> Case {
    Case {
        label: label.into(),
        family,
        bytes: raw(&serde_json::to_vec(&value).unwrap()),
        expected: Some((accept, usize::from(accept))),
    }
}
fn table() -> Value {
    json!({"database":vec![2u8;16],"timeline":1,"object":1,"generation":1,"schema_epoch":1})
}
fn row_id() -> Value {
    json!({"table":table(),"row":1})
}
fn txn() -> Value {
    json!({"database":vec![2u8;16],"timeline":1,"writer_epoch":1,"local":1})
}
fn spec() -> Value {
    json!({"name":"seed","columns":[{"name":"value","data_type":"int64","nullable":true}]})
}
fn cells() -> Value {
    json!([{"type":"null"},{"type":"bool","value":false},{"type":"bool","value":true},{"type":"int64","value":i64::MIN},{"type":"int64","value":i64::MAX},{"type":"utf8","value":"한글\0\n\\\""},{"type":"utf8","value":""},{"type":"binary","value":[0,1,127,128,255]},{"type":"binary","value":[]}])
}
fn row() -> Value {
    json!({"identity":row_id(),"values":cells()})
}
fn plan(target: &str, boundary: &str) -> Value {
    json!({"cut":{"target":target,"boundary":boundary},"occurrence":1,"actor":0,"directory":0})
}
fn witness(target: &str, boundary: &str) -> Value {
    json!({"event":"reached","witness":{"plan":plan(target,boundary),"matched":1,"actor":0,"request":1,"directory":0,"phase":if target=="recovery_old_file"{"open"}else{"commit"},"observed_target":target,"file":"wal-00000001-0000000000000001.axf","offset":100,"requested_bytes":50,"completed_bytes":if boundary=="write_partial"{1}else{50},"frame_start":100,"frame_end":150,"frame_written_bytes":if boundary=="write_partial"{1}else{50}}})
}
fn metrics(full: bool) -> Value {
    let operations = if full {
        json!(METRIC_IDS.iter().map(|id|json!({"metric_id":id,"attempts":u64::MAX,"successes":u64::MAX,"failures":u64::MAX,"requested_bytes":u64::MAX,"confirmed_complete_bytes":u64::MAX,"items":u64::MAX,"unknown_partial_operations":u64::MAX,"timing_missing":u64::MAX,"elapsed":{"samples":u64::MAX,"sum_ns":u64::MAX,"max_ns":u64::MAX,"log2_ns":vec![u64::MAX;64]}})).collect::<Vec<_>>())
    } else {
        Value::Null
    };
    json!({"semantic_revision":1,"operations":operations,"clock_configured":full,"clock_invalid":false,"counters_saturated":full,"gauges":{"retained_wal_bytes":0,"durable_position":0,"checkpoint_position":0,"control_reserved_bytes":0,"control_used_bytes":0,"oldest_system_pin_age_ns":null,"system_pins_over_60_seconds":null}})
}
fn corpus(lane: Lane) -> Vec<Case> {
    let mut bodies: Vec<(String, Value)> = Vec::new();
    let mut push = |label: &str, value: Value| bodies.push((label.into(), value));
    match lane {
        Lane::Request => {
            push(
                "configure_engine",
                json!({"operation":"configure_engine","observations":true,"clock":false}),
            );
            push(
                "configure_fault",
                json!({"operation":"configure_fault","token":vec![2u8;16],"plan":plan("wal_commit","sync_after")}),
            );
            for operation in ["create_database", "open_database", "backup"] {
                push(operation, json!({"operation":operation,"directory":7}));
            }
            for operation in [
                "close_database",
                "checkpoint",
                "collect_garbage",
                "metrics",
                "durability_metrics",
            ] {
                push(operation, json!({"operation":operation}));
            }
            push(
                "restore_database",
                json!({"operation":"restore_database","source":0,"target":7}),
            );
            push(
                "configure_gate",
                json!({"operation":"configure_gate","gate":32,"actors":[0,31]}),
            );
            push(
                "create_table",
                json!({"operation":"create_table","table":1024,"spec":spec()}),
            );
            push("drop_table", json!({"operation":"drop_table","table":1024}));
            for mode in ["read_only", "read_write"] {
                push(
                    &format!("begin_{mode}"),
                    json!({"operation":"begin","transaction":1024,"mode":mode}),
                );
            }
            push(
                "lookup",
                json!({"operation":"lookup","transaction":1,"table":1024,"name":"seed"}),
            );
            push(
                "bind_row",
                json!({"operation":"bind_row","row":1024,"identity":row_id()}),
            );
            push(
                "statement",
                json!({"operation":"statement","transaction":1,"mutations":[{"mutation":"insert","table":1,"row":1,"values":cells()},{"mutation":"replace","table":1,"row":2,"values":cells()},{"mutation":"delete","table":1,"row":3}]}),
            );
            push(
                "statement_empty",
                json!({"operation":"statement","transaction":1,"mutations":[]}),
            );
            push(
                "statement_limit",
                json!({"operation":"statement","transaction":1,"mutations":vec![json!({"mutation":"delete","table":1,"row":1});256]}),
            );
            push(
                "read",
                json!({"operation":"read","transaction":1,"table":1,"row":1}),
            );
            push(
                "open_scan",
                json!({"operation":"open_scan","transaction":1,"table":1,"cursor":1}),
            );
            push(
                "next_page",
                json!({"operation":"next_page","cursor":1024,"limit":1024}),
            );
            push(
                "close_scan",
                json!({"operation":"close_scan","cursor":1024}),
            );
            for operation in ["commit", "rollback", "cancel"] {
                push(operation, json!({"operation":operation,"transaction":1024}));
            }
        }
        Lane::Response => {
            push(
                "ready",
                json!({"result":"ready","database":vec![2u8;16],"timeline":1}),
            );
            push("done", json!({"result":"done"}));
            for wrote in [false, true] {
                push(
                    if wrote {
                        "table_create"
                    } else {
                        "table_lookup"
                    },
                    json!({"result":"table","identity":table(),"spec":spec(),"commit":if wrote{json!({"transaction":txn(),"position":1,"wrote":true})}else{Value::Null}}),
                );
            }
            push("begun", json!({"result":"begun","identity":txn()}));
            push(
                "statement",
                json!({"result":"statement","inserted":[{"alias":1,"identity":row_id()}],"affected_rows":3}),
            );
            push("row_some", json!({"result":"row","value":row()}));
            push("row_none", json!({"result":"row","value":null}));
            push(
                "page",
                json!({"result":"page","rows":[row()],"eof":false,"canonical_bytes":262144}),
            );
            push(
                "page_empty",
                json!({"result":"page","rows":[],"eof":true,"canonical_bytes":0}),
            );
            for wrote in [false, true] {
                push(
                    if wrote {
                        "committed_write"
                    } else {
                        "committed_read"
                    },
                    json!({"result":"committed","transaction":txn(),"position":1,"wrote":wrote}),
                );
            }
            for result in ["checkpoint", "backup"] {
                push(result, json!({"result":result,"generation":1,"position":1}));
            }
            push(
                "garbage_collected",
                json!({"result":"garbage_collected","removed":0}),
            );
            push(
                "metrics",
                json!({"result":"metrics","counters":{"operations":0,"rows_read":0,"row_references_copied":0,"resident_bytes":0,"snapshot_pins":0,"private_bytes":0,"active_transactions":0,"oldest_user_pin_age_ns":null,"user_pins_over_sixty_seconds":null,"coordinator_acquisitions":0,"coordinator_wait_ns":null,"coordinator_hold_ns":null,"coordinator_clock_invalid":false,"counters_saturated":false,"serialization_conflicts":0,"statement_rollbacks":0,"commit_preparation_failures":0}}),
            );
            push(
                "durability_metrics_off",
                json!({"result":"durability_metrics","counters":metrics(false)}),
            );
            push(
                "durability_metrics_full",
                json!({"result":"durability_metrics","counters":metrics(true)}),
            );
            for code in [
                "XADB-ENGINE-0001",
                "XADB-ENGINE-0002",
                "XADB-ENGINE-0003",
                "XADB-ENGINE-0004",
                "XADB-ENGINE-0005",
                "XADB-ENGINE-0006",
                "XADB-ENGINE-0007",
                "XADB-ENGINE-0008",
                "XADB-ENGINE-0009",
                "ReadOnly",
                "SerializationConflict",
                "Cancelled",
                "Poisoned",
                "Fenced",
                "Closed",
            ] {
                push(
                    &format!("transaction_error_{code}"),
                    json!({"result":"error","failure":{"domain":"transaction","code":code,"effect":"unknown","certainty":"unknown","retry":"after_recovery","transaction":txn()}}),
                );
            }
            for code in [
                "InvalidInput",
                "Corruption",
                "UnsupportedFormat",
                "LimitExceeded",
                "QuotaExceeded",
                "Io",
                "Busy",
                "Fenced",
                "IncompleteCreate",
                "Transaction",
            ] {
                push(
                    &format!("storage_error_{code}"),
                    json!({"result":"error","failure":{"domain":"storage","code":code,"outcome":"unknown","fenced":true,"transaction":null}}),
                );
            }
        }
        Lane::FaultObservation => {
            push("connected", json!({"event":"connected","process_id":1}));
            for decision in ["continue", "return_error"] {
                push(
                    &format!("released_{decision}"),
                    json!({"event":"released","decision":decision}),
                );
            }
            let mut count = 0;
            for target in TARGETS {
                for boundary in BOUNDARIES {
                    if allowed(target, boundary) {
                        push(
                            &format!("reached_{target}_{boundary}"),
                            witness(target, boundary),
                        );
                        count += 1;
                    }
                }
            }
            assert_eq!(count, 42);
        }
        Lane::FaultRelease => {
            push("continue", json!("continue"));
            push("return_error", json!("return_error"));
        }
    }
    let expected = [28, 43, 45, 2][lane.index()];
    assert_eq!(bodies.len(), expected, "frozen corpus size {}", lane.name());
    bodies
        .into_iter()
        .map(|(label, body)| literal(label, "valid", lane.wrap(body), true))
        .collect()
}
fn parsed(input: &Case) -> Value {
    serde_json::from_slice(&input.bytes[4..]).unwrap()
}
fn negatives(lane: Lane, seeds: &[Case]) -> Vec<Case> {
    let mut result = Vec::new();
    let base = parsed(&seeds[0]);
    for (label, field, value) in [
        ("version", "version", json!(2)),
        ("zero_run", "run", json!(vec![0u8; 16])),
    ] {
        let mut value_case = base.clone();
        value_case[field] = value;
        result.push(literal(label, "identity", value_case, false));
    }
    match lane {
        Lane::Request | Lane::Response => {
            for (label, field, value) in [
                ("request_zero", "request", json!(0)),
                ("request_high", "request", json!(1025)),
                ("actor_high", "actor", json!(32)),
            ] {
                let mut input = base.clone();
                input[field] = value;
                result.push(literal(label, "identity", input, false));
            }
        }
        _ => {
            let mut input = base.clone();
            input["token"] = json!(vec![0u8; 16]);
            result.push(literal("token_zero", "identity", input, false));
        }
    }
    let mut push = |label: &str, family: &'static str, body: Value| {
        result.push(literal(label, family, lane.wrap(body), false))
    };
    match lane {
        Lane::Request => {
            for directory in [8, 255] {
                push(
                    "directory_high",
                    "directory",
                    json!({"operation":"open_database","directory":directory}),
                );
            }
            push(
                "restore_same",
                "directory",
                json!({"operation":"restore_database","source":1,"target":1}),
            );
            for alias in [0, 1025] {
                push(
                    "alias_bound",
                    "alias",
                    json!({"operation":"commit","transaction":alias}),
                );
            }
            for actors in [
                json!([0]),
                json!([0, 0]),
                json!([0, 32]),
                json!(vec![0; 33]),
            ] {
                push(
                    "gate_participants",
                    "gate",
                    json!({"operation":"configure_gate","gate":1,"actors":actors}),
                );
            }
            push(
                "gate_id",
                "gate",
                json!({"operation":"configure_gate","gate":0,"actors":[0,1]}),
            );
            push(
                "statement_257",
                "statement",
                json!({"operation":"statement","transaction":1,"mutations":vec![json!({"mutation":"delete","table":1,"row":1});257]}),
            );
            push(
                "statement_alias",
                "statement",
                json!({"operation":"statement","transaction":1,"mutations":[{"mutation":"delete","table":0,"row":1}]}),
            );
            for limit in [0, 1025] {
                push(
                    "page_limit",
                    "page",
                    json!({"operation":"next_page","cursor":1,"limit":limit}),
                );
            }
            push(
                "fault_token",
                "fault_plan",
                json!({"operation":"configure_fault","token":vec![0u8;16],"plan":plan("any","create_before")}),
            );
        }
        Lane::Response => {
            for domain in ["transaction", "storage"] {
                let body = if domain == "transaction" {
                    json!({"result":"error","failure":{"domain":domain,"code":"NotACode","effect":"aborted","certainty":"known","retry":"never","transaction":null}})
                } else {
                    json!({"result":"error","failure":{"domain":domain,"code":"NotACode","outcome":"aborted","fenced":false,"transaction":null}})
                };
                push("unknown_product_error", "error_catalog", body);
            }
            for kind in 0..6 {
                let mut counters = metrics(true);
                match kind {
                    0 => counters["semantic_revision"] = json!(2),
                    1 => {
                        counters["operations"].as_array_mut().unwrap().pop();
                    }
                    2 => {
                        let duplicate = counters["operations"][0].clone();
                        counters["operations"][1] = duplicate;
                    }
                    3 => counters["operations"][0]["metric_id"] = json!("not.a.metric"),
                    4 => {
                        counters["operations"][0]["elapsed"]["log2_ns"]
                            .as_array_mut()
                            .unwrap()
                            .pop();
                    }
                    5 => counters["operations"][0]["elapsed"]["log2_ns"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!(0)),
                    _ => unreachable!(),
                }
                push(
                    "invalid_metrics",
                    "metrics",
                    json!({"result":"durability_metrics","counters":counters}),
                );
            }
        }
        Lane::FaultObservation => {
            push(
                "zero_pid",
                "witness",
                json!({"event":"connected","process_id":0}),
            );
            for target in TARGETS {
                for boundary in BOUNDARIES {
                    if !allowed(target, boundary) {
                        push(
                            &format!("unsupported_{target}_{boundary}"),
                            "cut_catalog",
                            witness(target, boundary),
                        );
                    }
                }
            }
            for (label, path, value) in [
                ("matched", "/event/witness/matched", json!(2)),
                ("actor", "/event/witness/actor", json!(1)),
                ("directory", "/event/witness/directory", json!(1)),
                ("request_zero", "/event/witness/request", json!(0)),
                ("request_high", "/event/witness/request", json!(1025)),
                ("target", "/event/witness/observed_target", json!("current")),
                ("filename", "/event/witness/file", json!("../file")),
                ("partial_zero", "/event/witness/completed_bytes", json!(0)),
                ("partial_two", "/event/witness/completed_bytes", json!(2)),
                (
                    "partial_request_one",
                    "/event/witness/requested_bytes",
                    json!(1),
                ),
                ("partial_no_offset", "/event/witness/offset", Value::Null),
                (
                    "occurrence_zero",
                    "/event/witness/plan/occurrence",
                    json!(0),
                ),
                (
                    "occurrence_high",
                    "/event/witness/plan/occurrence",
                    json!(4097),
                ),
                ("filter_actor_high", "/event/witness/plan/actor", json!(32)),
                (
                    "filter_directory_high",
                    "/event/witness/plan/directory",
                    json!(8),
                ),
            ] {
                let mut input = lane.wrap(witness("wal_commit", "write_partial"));
                *input.pointer_mut(path).unwrap() = value;
                result.push(literal(label, "witness", input, false));
            }
            for (path, value) in [
                ("/event/witness/frame_end", json!(99)),
                ("/event/witness/frame_end", json!(100)),
                ("/event/witness/frame_end", Value::Null),
                ("/event/witness/frame_written_bytes", json!(49)),
            ] {
                let mut input = lane.wrap(witness("wal_commit", "sync_after"));
                *input.pointer_mut(path).unwrap() = value;
                result.push(literal("incomplete_wal", "witness", input, false));
            }
            let mut input = lane.wrap(witness("recovery_old_file", "sync_after"));
            input["event"]["witness"]["phase"] = json!("commit");
            result.push(literal("recovery_phase", "witness", input, false));
        }
        Lane::FaultRelease => {
            push("unknown_decision", "tag", json!("maybe"));
        }
    }
    if matches!(lane, Lane::Request) {
        for gate in [0, 1, 33] {
            let mut input = base.clone();
            input["command"]["gate"] = json!(gate);
            result.push(literal("exclusive_gate", "gate", input, false));
        }
    }
    if matches!(lane, Lane::Response) {
        for gate in [0, 33] {
            let mut input = base.clone();
            input["passed_gate"] = json!(gate);
            result.push(literal("response_gate", "gate", input, false));
        }
    }
    result
}
fn strict(lane: Lane, seeds: &[Case]) -> Vec<Case> {
    let mut result = negatives(lane, seeds);
    let short = &seeds[0].bytes;
    let mut add =
        |label: String, family: &'static str, bytes: Vec<u8>, accept: bool, count: usize| {
            result.push(Case {
                label,
                family,
                bytes,
                expected: Some((accept, count)),
            })
        };
    add("empty_stream".into(), "eof", vec![], true, 0);
    // Every strict nonempty prefix of one fixed short literal, never all prefixes
    // of the 4 MiB vectors below. This range is independent of the codec result.
    assert!(short.len() < 512);
    for end in 1..short.len() {
        add(
            format!("short_prefix_{end}"),
            "truncation",
            short[..end].to_vec(),
            false,
            0,
        );
    }
    for length in [0, MAX_BODY_BYTES as u32 + 1, u32::MAX] {
        add(
            format!("length_{length}"),
            "length",
            length.to_be_bytes().to_vec(),
            false,
            0,
        );
    }
    let text = std::str::from_utf8(&short[4..]).unwrap();
    for (name, body) in [
        ("unknown", format!("{{\"unexpected\":1,{}", &text[1..])),
        ("duplicate", format!("{{\"version\":1,{}", &text[1..])),
        ("trailing_json", format!("{text}{{}}")),
    ] {
        add(name.into(), "json", raw(body.as_bytes()), false, 0);
    }
    add(
        "invalid_utf8".into(),
        "utf8",
        raw(&[b'{', 0xff, b'}']),
        false,
        0,
    );
    let mut maximum = text.as_bytes().to_vec();
    maximum.resize(MAX_BODY_BYTES, b' ');
    add("max_frame".into(), "boundary", raw(&maximum), true, 1);
    maximum.push(b' ');
    add("over_max_frame".into(), "boundary", raw(&maximum), false, 0);
    let mut concat = short.clone();
    concat.extend_from_slice(short);
    add("two_frames".into(), "concatenation", concat, true, 2);
    let mut suffix = short.clone();
    suffix.extend_from_slice(&[0, 0, 0]);
    add("partial_suffix".into(), "concatenation", suffix, false, 1);
    let mut suffix = short.clone();
    suffix.extend_from_slice(&[0, 0, 0, 0]);
    add("invalid_suffix".into(), "concatenation", suffix, false, 1);
    result
}
fn mutate(lane: Lane, seeds: &[Case], random: &mut Random, operator: usize) -> Case {
    let seed = &seeds[random.index(seeds.len())];
    let mut bytes = seed.bytes.clone();
    let mut expected = None;
    match operator {
        0 => {
            let mut input = parsed(seed);
            input["run"] = json!(vec![(random.index(255) + 1) as u8; 16]);
            if matches!(lane, Lane::Request | Lane::Response) {
                input["request"] = json!(random.index(1024) + 1);
                input["actor"] = json!(random.index(32));
            } else {
                input["token"] = json!(vec![(random.index(255) + 1) as u8; 16]);
            }
            bytes = raw(&serde_json::to_vec(&input).unwrap());
            expected = Some((true, 1));
        }
        1 => {
            let index = random.index(bytes.len());
            bytes[index] ^= 1 << random.index(8);
        }
        2 => {
            let index = random.index(bytes.len() + 1);
            bytes.insert(index, random.next() as u8);
        }
        3 => {
            let index = random.index(bytes.len());
            bytes.remove(index);
        }
        4 => bytes[..4].copy_from_slice(&(random.next() as u32).to_be_bytes()),
        5 => {
            let end = random.index(bytes.len());
            bytes.truncate(end);
        }
        6 => {
            bytes.extend_from_slice(&seeds[random.index(seeds.len())].bytes);
            expected = Some((true, 2));
        }
        7 | 8 => {
            let body = std::str::from_utf8(&bytes[4..]).unwrap();
            bytes = raw(format!(
                "{{{}{}",
                if operator == 7 {
                    "\"unexpected\":0,"
                } else {
                    "\"version\":1,"
                },
                &body[1..]
            )
            .as_bytes());
            expected = Some((false, 0));
        }
        9 => {
            let mut value = parsed(seed);
            match lane {
                Lane::Request => value["command"]["action"]["operation"] = json!("unknown"),
                Lane::Response => value["observation"]["result"] = json!("unknown"),
                Lane::FaultObservation => value["event"]["event"] = json!("unknown"),
                Lane::FaultRelease => value["decision"] = json!("unknown"),
            };
            bytes = raw(&serde_json::to_vec(&value).unwrap());
            expected = Some((false, 0));
        }
        10 => {
            let mut value = parsed(seed);
            value["version"] = json!(u64::MAX);
            bytes = raw(&serde_json::to_vec(&value).unwrap());
            expected = Some((false, 0));
        }
        11 => {
            let index = 4 + random.index(bytes.len() - 4);
            bytes[index] = 0xff;
            expected = Some((false, 0));
        }
        12 | 13 => {
            let body = std::str::from_utf8(&bytes[4..]).unwrap();
            // Release has no nested object: its fallback is the envelope object.
            // Other lanes insert inside command/observation/event, preserving
            // the original member so duplicate rejection is actually exercised.
            let mut position = body[1..].find('{').map_or(1, |index| index + 2);
            if body.as_bytes()[position] == b'}' {
                position = 1;
            }
            let addition = if operator == 12 {
                "\"nested_unknown\":{},".to_owned()
            } else {
                let key_end = body[position + 1..].find('"').unwrap() + position + 1;
                format!("{}:null,", &body[position..=key_end])
            };
            let changed = format!("{}{}{}", &body[..position], addition, &body[position..]);
            bytes = raw(changed.as_bytes());
            expected = Some((false, 0));
        }
        14 => {
            let mut body = bytes[4..].to_vec();
            body.extend_from_slice(b" true");
            bytes = raw(&body);
            expected = Some((false, 0));
        }
        15 => {
            bytes.extend_from_slice(&[0, 0, 0]);
            expected = Some((false, 1));
        }
        16 => {
            bytes.extend_from_slice(&[0, 0, 0, 0]);
            expected = Some((false, 1));
        }
        _ => unreachable!(),
    }
    assert!(bytes.len() <= INPUT_CAP);
    Case {
        label: format!("{}:{}", seed.label, OPERATORS[operator]),
        family: "mutation",
        bytes,
        expected,
    }
}
fn check<T: Envelope + Debug + PartialEq>(input: &Case, random: &mut Random) -> bool {
    assert!(input.bytes.len() <= INPUT_CAP);
    let mut reader = Cursor::new(&input.bytes);
    let mut blocking = Vec::new();
    let blocking_complete = loop {
        match frame::read::<T>(&mut reader) {
            Ok(Some(message)) => blocking.push(message),
            Ok(None) => break true,
            Err(_) => break false,
        }
    };
    let mut decoder = Decoder::<T>::new();
    let mut streaming = Vec::new();
    let mut position = 0;
    let mut streaming_complete = true;
    while position < input.bytes.len() {
        let end = (position + 1 + random.index(8192)).min(input.bytes.len());
        match decoder.feed(&input.bytes[position..end]) {
            Ok((used, message)) => {
                assert!(used > 0 && used <= end - position);
                position += used;
                if let Some(message) = message {
                    streaming.push(message);
                }
            }
            Err(_) => {
                streaming_complete = false;
                assert!(decoder.feed(&[]).is_err());
                break;
            }
        }
    }
    if streaming_complete {
        streaming_complete = decoder.finish().is_ok();
    }
    assert!(decoder.feed(&[]).is_err());
    assert!(decoder.finish().is_err());
    assert_eq!(blocking_complete, streaming_complete, "{}", input.label);
    assert_eq!(blocking, streaming, "{}", input.label);
    if let Some((complete, count)) = input.expected {
        assert_eq!(
            (blocking_complete, blocking.len()),
            (complete, count),
            "{}",
            input.label
        );
    }
    for message in blocking {
        let encoded = frame::encode(&message).unwrap();
        assert!(encoded.len() <= MAX_FRAME_BYTES);
        let decoded = frame::read::<T>(&mut Cursor::new(&encoded))
            .unwrap()
            .unwrap();
        assert_eq!(message, decoded);
    }
    blocking_complete
}
fn dispatch(lane: Lane, input: &Case, random: &mut Random) -> bool {
    match lane {
        Lane::Request => check::<durable::Request>(input, random),
        Lane::Response => check::<durable::Response>(input, random),
        Lane::FaultObservation => check::<fault::Observation>(input, random),
        Lane::FaultRelease => check::<fault::Release>(input, random),
    }
}
fn checked(
    lane: Lane,
    input: &Case,
    seed: u64,
    round: u64,
    case_index: u64,
    random: &mut Random,
) -> bool {
    let chunk_state = random.0;
    match catch_unwind(AssertUnwindSafe(|| dispatch(lane, input, random))) {
        Ok(complete) => complete,
        Err(failure) => {
            let receipt = json!({"target":TARGET,"generator":GENERATOR,"lane":lane.name(),"seed":seed,"round":round,"case_index":case_index,"chunk_state":chunk_state,"label":input.label,"family":input.family,"expected":input.expected,"input_bytes":input.bytes.len()});
            let parent = std::env::var_os("XADBTEST_V3_FAILURE_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir);
            let directory = parent.join(format!(
                "{TARGET}-{}-{}-{case_index}",
                lane.name(),
                std::process::id()
            ));
            let saved = (|| -> std::io::Result<()> {
                fs::create_dir_all(&parent)?;
                fs::create_dir(&directory)?;
                let mut raw = OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(directory.join("input.bin"))?;
                raw.write_all(&input.bytes)?;
                raw.sync_all()?;
                let mut meta = OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(directory.join("receipt.json"))?;
                serde_json::to_writer_pretty(&mut meta, &receipt)?;
                meta.sync_all()?;
                Ok(())
            })();
            if let Err(error) = saved {
                eprintln!(
                    "INCOMPLETE failure preservation: {error}; receipt={receipt}; input={:02x?}",
                    input.bytes
                );
            } else {
                eprintln!("retained mutation failure {}", directory.display());
            }
            resume_unwind(failure)
        }
    }
}
#[derive(Default)]
struct Counts {
    cases: u64,
    accepted: u64,
    rejected: u64,
    known_valid: u64,
    known_invalid: u64,
    variants: BTreeMap<String, u64>,
    families: BTreeMap<String, u64>,
    operators: BTreeMap<String, u64>,
}
impl Counts {
    fn run(&mut self, lane: Lane, input: &Case, seed: u64, round: u64, random: &mut Random) {
        let complete = checked(lane, input, seed, round, self.cases, random);
        self.cases += 1;
        if complete {
            self.accepted += 1
        } else {
            self.rejected += 1
        }
        match input.expected {
            Some((true, _)) => self.known_valid += 1,
            Some((false, _)) => self.known_invalid += 1,
            None => (),
        }
        *self.families.entry(input.family.into()).or_default() += 1;
    }
}
fn replay() -> bool {
    let Some(path) = std::env::var_os("XADBTEST_V3_REPLAY_RECEIPT") else {
        return false;
    };
    let path = PathBuf::from(path);
    let receipt: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(receipt["target"], TARGET);
    assert_eq!(receipt["generator"], GENERATOR);
    let lane = LANES
        .into_iter()
        .find(|lane| receipt["lane"] == lane.name())
        .expect("known lane");
    let bytes = fs::read(path.parent().unwrap().join("input.bin")).unwrap();
    assert!(bytes.len() <= INPUT_CAP);
    let expected = if receipt["expected"].is_null() {
        None
    } else {
        Some((
            receipt["expected"][0].as_bool().unwrap(),
            receipt["expected"][1].as_u64().unwrap() as usize,
        ))
    };
    let input = Case {
        label: receipt["label"].as_str().unwrap().into(),
        family: "replay",
        bytes,
        expected,
    };
    checked(
        lane,
        &input,
        receipt["seed"].as_u64().unwrap(),
        receipt["round"].as_u64().unwrap(),
        receipt["case_index"].as_u64().unwrap(),
        &mut Random(receipt["chunk_state"].as_u64().unwrap()),
    );
    true
}
fn run(long: bool) {
    if !long && replay() {
        return;
    }
    let seed_count = if long { SEEDS } else { 64 };
    let operations_per_seed = if long { 1024 } else { 128 };
    let minimum = if long { MIN_SECONDS } else { 0 };
    let corpora: Vec<_> = LANES.iter().map(|lane| corpus(*lane)).collect();
    let strict_cases: Vec<_> = LANES
        .iter()
        .enumerate()
        .map(|(index, lane)| strict(*lane, &corpora[index]))
        .collect();
    let literal_profile:Vec<_>=LANES.iter().enumerate().map(|(index,lane)|{
        let mut families:BTreeMap<&str,usize>=BTreeMap::new();for input in &strict_cases[index]{*families.entry(input.family).or_default()+=1;}
        json!({"lane":lane.name(),"valid":corpora[index].iter().map(|input|input.label.as_str()).collect::<Vec<_>>(),"strict_count":strict_cases[index].len(),"strict_families":families})
    }).collect();
    println!(
        "XADB_V3_PROFILE {}",
        json!({"target":TARGET,"generator":GENERATOR,"seed":0,"seed_count":seed_count,"operations_per_seed":operations_per_seed,"min_seconds":minimum,"round_cap":ROUND_CAP,"input_cap":INPUT_CAP,"lanes":literal_profile,"operators":OPERATORS,"coverage_guided":false,"independent_oracle":"literal_acceptance_only","fault_transport_caps_included":false})
    );
    let mut counts: [Counts; 4] = std::array::from_fn(|_| Counts::default());
    for (index, lane) in LANES.into_iter().enumerate() {
        let mut random = Random(0x5841_4442_4455_5231 ^ index as u64);
        for input in &corpora[index] {
            counts[index].run(lane, input, 0, 0, &mut random);
            *counts[index]
                .variants
                .entry(input.label.clone())
                .or_default() += 1;
        }
        for input in &strict_cases[index] {
            counts[index].run(lane, input, 0, 0, &mut random);
        }
    }
    let mut coverage: Vec<Vec<u64>> = (0..4)
        .map(|_| vec![0; seed_count * OPERATORS.len()])
        .collect();
    // The minimum measures only mutation work, after fixed literal setup.
    let started = Instant::now();
    let mut progress = Instant::now();
    let mut rounds = 0u64;
    let coverage_period = (seed_count * OPERATORS.len()) as u64;
    let minimum_rounds = (seed_count * operations_per_seed) as u64;
    while rounds < minimum_rounds || started.elapsed() < Duration::from_secs(minimum) {
        assert!(
            rounds < ROUND_CAP,
            "INCOMPLETE round cap before minimum duration"
        );
        let seed = (rounds % seed_count as u64) as usize;
        let operator = (rounds % OPERATORS.len() as u64) as usize;
        for (index, lane) in LANES.into_iter().enumerate() {
            let mut random = Random(
                0x5841_4442_4455_5231
                    ^ (seed as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
                    ^ ((index as u64) << 32)
                    ^ rounds.rotate_left(17),
            );
            let input = mutate(lane, &corpora[index], &mut random, operator);
            counts[index].run(lane, &input, seed as u64, rounds, &mut random);
            coverage[index][seed * OPERATORS.len() + operator] += 1;
            *counts[index]
                .operators
                .entry(OPERATORS[operator].into())
                .or_default() += 1;
        }
        rounds += 1;
        if progress.elapsed() >= Duration::from_secs(30) {
            println!(
                "XADB_V3_PROGRESS {}",
                json!({"target":TARGET,"rounds":rounds,"mutations":4*rounds,"elapsed_ms":started.elapsed().as_millis()})
            );
            progress = Instant::now();
        }
    }
    let elapsed = started.elapsed();
    assert!(elapsed >= Duration::from_secs(minimum));
    let mut lanes = serde_json::Map::new();
    for (index, lane) in LANES.into_iter().enumerate() {
        let count = &counts[index];
        assert_eq!(
            count.cases,
            (corpora[index].len() + strict_cases[index].len()) as u64 + rounds
        );
        assert_eq!(count.accepted + count.rejected, count.cases);
        assert!(
            count.accepted > 0
                && count.rejected > 0
                && count.known_valid > 0
                && count.known_invalid > 0
        );
        assert_eq!(count.variants.len(), corpora[index].len());
        assert_eq!(count.operators.len(), OPERATORS.len());
        assert!(coverage[index].iter().all(|visits| *visits > 0));
        assert_eq!(coverage[index].iter().sum::<u64>(), rounds);
        let mut expected = vec![rounds / coverage_period; seed_count * OPERATORS.len()];
        for remainder in 0..(rounds % coverage_period) as usize {
            expected[(remainder % seed_count) * OPERATORS.len() + remainder % OPERATORS.len()] += 1;
        }
        assert_eq!(coverage[index], expected);
        let seed_visits: Vec<u64> = coverage[index]
            .chunks_exact(OPERATORS.len())
            .map(|visits| visits.iter().sum())
            .collect();
        assert!(
            seed_visits
                .iter()
                .all(|visits| *visits >= operations_per_seed as u64)
        );
        lanes.insert(lane.name().into(),json!({"cases":count.cases,"accepted":count.accepted,"rejected":count.rejected,"known_valid":count.known_valid,"known_invalid":count.known_invalid,"mutations":rounds,"variant_visits":count.variants,"family_visits":count.families,"operator_visits":count.operators,"seed_visits":seed_visits,"seed_operator_pairs":coverage[index].len(),"seed_operator_min_visits":coverage[index].iter().min().unwrap()}));
    }
    println!(
        "XADB_V3_RESULT {}",
        json!({"target":TARGET,"generator":GENERATOR,"seed":0,"seed_count":seed_count,"operations_per_seed":operations_per_seed,"elapsed_ms":elapsed.as_millis(),"rounds":rounds,"mutations":4*rounds,"missing_lanes":0,"lane_counts":lanes})
    );
}
#[test]
fn durable_and_fault_mutation_corpus_smoke() {
    run(false);
}
#[test]
#[ignore = "explicit V3 four-codec mutation fuzz: fixed mutation-loop minimum 1800 seconds"]
fn v3_durable_and_fault_codecs_mutation_fuzz_30_minutes() {
    run(true);
}
