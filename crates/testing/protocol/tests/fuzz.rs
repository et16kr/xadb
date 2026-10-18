//! Reproducible, bounded mutation fuzzing of the native Request/Response codecs.
//! This is not coverage-guided fuzzing. Differential cases compare two framing
//! paths that share JSON decoding; only the literal cases have an independent
//! acceptance oracle. Product/worker semantics are deliberately outside scope.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fmt::Debug,
    fs,
    io::Cursor,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    path::PathBuf,
    time::{Duration, Instant},
};
use xadbtest_protocol::{
    engine::{Request, Response},
    frame::{self, Decoder, Envelope, MAX_BODY_BYTES, MAX_FRAME_BYTES},
};

const GENERATOR: &str = "k0-native-mutation-v1";
const MIN_SECONDS: u64 = 1800;
const INPUT_CAP: usize = 2 * MAX_FRAME_BYTES + 4;
const REQUEST_SEED: u64 = 0x5841_4442_5245_5131;
const RESPONSE_SEED: u64 = 0x5841_4442_5245_5331;
const OPERATORS: [&str; 17] = [
    "valid_identity_text",
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
const IDENTITY: &str =
    r#""version":1,"run":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],"request":1,"actor":0"#;
const TABLE: &str = r#"{"database":[2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2],"timeline":1,"object":1,"generation":1,"schema_epoch":1}"#;
const SPEC: &str = r#"{"name":"seed","columns":[{"name":"b","data_type":"bool","nullable":true},{"name":"i","data_type":"int64","nullable":false},{"name":"s","data_type":"utf8","nullable":true},{"name":"x","data_type":"binary","nullable":true}]}"#;
const CELLS: &str = r#"[{"type":"null"},{"type":"bool","value":false},{"type":"bool","value":true},{"type":"int64","value":-9223372036854775808},{"type":"int64","value":9223372036854775807},{"type":"utf8","value":"seed\u0000\n한글\\\""},{"type":"utf8","value":""},{"type":"binary","value":[0,1,127,128,255]},{"type":"binary","value":[]}]"#;
const METRICS: &str = r#"{"operations":0,"rows_read":0,"row_references_copied":0,"resident_bytes":0,"snapshot_pins":0}"#;

#[derive(Clone, Copy)]
enum Target {
    Request,
    Response,
}
impl Target {
    fn name(self) -> &'static str {
        match self {
            Self::Request => "native_request_fuzz",
            Self::Response => "native_response_fuzz",
        }
    }
    fn seed(self) -> u64 {
        match self {
            Self::Request => REQUEST_SEED,
            Self::Response => RESPONSE_SEED,
        }
    }
    fn field(self) -> &'static str {
        match self {
            Self::Request => "action",
            Self::Response => "observation",
        }
    }
    fn tag(self) -> &'static str {
        match self {
            Self::Request => "operation",
            Self::Response => "result",
        }
    }
    fn wrap(self, body: &str) -> String {
        format!("{{{IDENTITY},\"{}\":{body}}}", self.field())
    }
}

// SplitMix64 state, constants and byte order are fixed parts of generator v1.
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

#[derive(Clone, Copy)]
enum Expect {
    Accept(usize),
    Reject(usize),
    Differential,
}
impl Expect {
    fn json(self) -> Value {
        match self {
            Self::Accept(count) => json!({"kind":"accept","prefix_messages":count}),
            Self::Reject(count) => json!({"kind":"reject","prefix_messages":count}),
            Self::Differential => json!({"kind":"differential"}),
        }
    }
}
struct Case {
    label: String,
    family: &'static str,
    bytes: Vec<u8>,
    expect: Expect,
    // Independently constructed oversized escaped JSON must also be rejected
    // by the capped encoder, even though its decoded string is much smaller.
    encoder_reject: bool,
}
fn raw(body: &[u8]) -> Vec<u8> {
    let mut bytes = u32::try_from(body.len()).unwrap().to_be_bytes().to_vec();
    bytes.extend_from_slice(body);
    bytes
}
fn case(label: impl Into<String>, family: &'static str, bytes: Vec<u8>, expect: Expect) -> Case {
    Case {
        label: label.into(),
        family,
        bytes,
        expect,
        encoder_reject: false,
    }
}

fn performance_input(mode: &str) -> String {
    format!(
        r#"{{"version":1,"seed":17,"row_count":1,"pin_old":true,"mode":"{mode}","capacity":1,"warmup":4096,"measure":65536,"latency_stride":16}}"#
    )
}
fn performance_output(mode: &str) -> String {
    let operations = if mode == "off" {
        "null".into()
    } else {
        format!(
            "[{}]",
            [r#"{"started":1,"completed":1,"failed":0}"#; 11].join(",")
        )
    };
    let diag = format!(
        r#"{{"mode":"{mode}","event_capacity":1,"logical_bytes":0,"operations":{operations},"dropped_events":{},"saturated":{}}}"#,
        if mode == "off" { "null" } else { "0" },
        if mode == "off" { "null" } else { "false" }
    );
    let events = [
        "create",
        "drop",
        "insert",
        "replace",
        "delete",
        "snapshot",
        "lookup",
        "schema",
        "read",
        "scan",
        "next_page",
    ]
    .iter()
    .enumerate()
    .map(|(index, operation)| {
        format!(
            r#"{{"sequence":{},"operation":"{operation}","error":{}}}"#,
            index + 1,
            if index == 0 {
                r#""XADB-ENGINE-0001""#
            } else {
                "null"
            }
        )
    })
    .collect::<Vec<_>>()
    .join(",");
    // This is a codec shape seed, not a valid completed measurement report.
    format!(
        r#"{{"version":1,"input":{},"path":"k0.replace_snapshot_read.v1","build":{{"source_sha256":"seed","toolchain":"1.95.0","target":"x86_64-unknown-linux-gnu","profile":"release","self_test":false}},"setup_ns":1,"warmup":{{"elapsed_ns":1,"completed":4096}},"measurement":{{"elapsed_ns":1,"completed":65536}},"error":null,"read_values":[-9223372036854775808,9223372036854775807],"final_values":[0],"old_values":[1],"latency_ns":[0,18446744073709551615],"metrics_before":{METRICS},"metrics_after":{METRICS},"diagnostics_before":{diag},"diagnostics_after":{diag},"events":[{events}],"events_available":true,"cpu_ns":null,"process_peak_rss_bytes":18446744073709551615}}"#,
        performance_input(mode)
    )
}

fn corpus(target: Target) -> Vec<(String, Vec<u8>)> {
    let row_id = format!(r#"{{"table":{TABLE},"row":1}}"#);
    let row = format!(r#"{{"identity":{row_id},"values":{CELLS}}}"#);
    let mut variants: Vec<(String, String)> = match target {
        Target::Request => vec![
            ("initialize".into(), r#"{"operation":"initialize","database":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],"timeline":1,"epoch":1,"resident_bytes":67108864,"snapshot_pins":32}"#.into()),
            ("create".into(), format!(r#"{{"operation":"create","table":1,"spec":{SPEC}}}"#)),
            ("lookup".into(), r#"{"operation":"lookup","snapshot":1,"table":1,"name":"seed"}"#.into()),
            ("drop_table".into(), r#"{"operation":"drop_table","table":1}"#.into()),
            ("insert".into(), format!(r#"{{"operation":"insert","table":1,"row":1,"values":{CELLS}}}"#)),
            ("replace".into(), format!(r#"{{"operation":"replace","table":1,"row":1,"values":{CELLS}}}"#)),
            ("delete".into(), r#"{"operation":"delete","table":1,"row":1}"#.into()),
            ("snapshot".into(), r#"{"operation":"snapshot","snapshot":1}"#.into()),
            ("release_snapshot".into(), r#"{"operation":"release_snapshot","snapshot":1}"#.into()),
            ("read".into(), r#"{"operation":"read","snapshot":1,"table":1,"row":1}"#.into()),
            ("open_scan".into(), r#"{"operation":"open_scan","snapshot":1,"table":1,"cursor":1}"#.into()),
            ("next_page".into(), r#"{"operation":"next_page","cursor":1,"limit":4294967295}"#.into()),
            ("close_scan".into(), r#"{"operation":"close_scan","cursor":1}"#.into()),
            ("metrics".into(), r#"{"operation":"metrics"}"#.into()),
        ],
        Target::Response => vec![
            ("ready".into(), r#"{"result":"ready"}"#.into()),
            ("done".into(), r#"{"result":"done"}"#.into()),
            ("table".into(), format!(r#"{{"result":"table","identity":{TABLE},"spec":{SPEC}}}"#)),
            ("inserted".into(), format!(r#"{{"result":"inserted","identity":{row_id}}}"#)),
            ("row_some".into(), format!(r#"{{"result":"row","value":{row}}}"#)),
            ("row_none".into(), r#"{"result":"row","value":null}"#.into()),
            ("page".into(), format!(r#"{{"result":"page","rows":[{row}],"eof":false,"canonical_bytes":262144}}"#)),
            ("empty_page".into(), r#"{"result":"page","rows":[],"eof":true,"canonical_bytes":0}"#.into()),
            ("metrics".into(), format!(r#"{{"result":"metrics","counters":{METRICS}}}"#)),
        ],
    };
    for mode in ["off", "always", "detailed"] {
        let body = match target {
            Target::Request => format!(
                r#"{{"operation":"performance","input":{}}}"#,
                performance_input(mode)
            ),
            Target::Response => format!(
                r#"{{"result":"performance","output":{}}}"#,
                performance_output(mode)
            ),
        };
        variants.push((format!("performance_{mode}"), body));
    }
    if matches!(target, Target::Response) {
        for code in 1..=9 {
            variants.push((
                format!("product_error_{code}"),
                format!(r#"{{"result":"product_error","code":"XADB-ENGINE-{code:04}"}}"#),
            ));
        }
    }
    variants
        .into_iter()
        .map(|(name, body)| (name, raw(target.wrap(&body).as_bytes())))
        .collect()
}

fn strict_cases(target: Target, seeds: &[(String, Vec<u8>)]) -> Vec<Case> {
    let good = &seeds[0].1;
    let body = std::str::from_utf8(&good[4..]).unwrap();
    let mut cases = vec![case("clean_eof", "eof", vec![], Expect::Accept(0))];
    for count in [0, MAX_BODY_BYTES + 1, u32::MAX as usize] {
        cases.push(case(
            format!("length_{count}"),
            "length",
            (count as u32).to_be_bytes().to_vec(),
            Expect::Reject(0),
        ));
    }
    cases.push(case("body_one", "length", raw(b"0"), Expect::Reject(0)));
    for (index, (from, to, family)) in [
        ("\"version\":1", "\"version\":0", "identity"),
        ("\"request\":1", "\"request\":0", "identity"),
        ("\"request\":1", "\"request\":1025", "identity"),
        ("\"actor\":0", "\"actor\":32", "identity"),
        ("\"actor\":0", "\"actor\":256", "identity"),
        ("\"version\":1", "\"version\":1,\"version\":1", "duplicate"),
        ("\"actor\":0", "\"actor\":0,\"verdict\":true", "unknown"),
        (
            "[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1]",
            "[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]",
            "identity",
        ),
        ("[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1]", "[1]", "identity"),
    ]
    .into_iter()
    .enumerate()
    {
        assert!(body.contains(from));
        cases.push(case(
            format!("strict_{index}"),
            family,
            raw(body.replace(from, to).as_bytes()),
            Expect::Reject(0),
        ));
    }
    for (label, body) in [
        ("null", b"null".as_slice()),
        ("json_tail", b"{}{}"),
        ("open_json", b"{"),
        ("utf8", b"\xff"),
        ("short_utf8", b"\xe2\x82"),
    ] {
        cases.push(case(
            label,
            if label.contains("utf8") {
                "utf8"
            } else {
                "json"
            },
            raw(body),
            Expect::Reject(0),
        ));
    }
    for (name, bytes) in seeds {
        let text = std::str::from_utf8(&bytes[4..]).unwrap();
        let needle = format!("\"{}\":{{", target.field());
        for (suffix, field, family) in [
            ("unknown", "\"verdict\":true,", "unknown"),
            ("duplicate", "\"version\":1,", "duplicate"),
        ] {
            let mutated = if suffix == "duplicate" {
                text.replacen('{', &format!("{{{field}"), 1)
            } else {
                text.replace(&needle, &format!("{needle}{field}"))
            };
            cases.push(case(
                format!("{name}_{suffix}"),
                family,
                raw(mutated.as_bytes()),
                Expect::Reject(0),
            ));
        }
        let tag = format!("\"{}\":\"", target.tag());
        cases.push(case(
            format!("{name}_tag"),
            "tag",
            raw(text.replace(&tag, &format!("{tag}unknown_")).as_bytes()),
            Expect::Reject(0),
        ));
    }
    for end in 1..good.len() {
        cases.push(case(
            format!("prefix_{end}"),
            "truncation",
            good[..end].to_vec(),
            Expect::Reject(0),
        ));
    }
    cases.push(case(
        "two_frames",
        "concatenation",
        [good.as_slice(), good.as_slice()].concat(),
        Expect::Accept(2),
    ));
    for suffix in [&good[..3], &[0, 0, 0, 0][..], raw(b"null").as_slice()] {
        cases.push(case(
            "valid_then_invalid",
            "concatenation",
            [good.as_slice(), suffix].concat(),
            Expect::Reject(1),
        ));
    }
    let mut maximum = body.as_bytes().to_vec();
    maximum.resize(MAX_BODY_BYTES, b' ');
    cases.push(case(
        "maximum_body",
        "length",
        raw(&maximum),
        Expect::Accept(1),
    ));
    maximum.push(b' ');
    cases.push(case(
        "maximum_plus_one",
        "length",
        raw(&maximum),
        Expect::Reject(0),
    ));
    let with_name = match target {
        Target::Request => r#"{"operation":"lookup","snapshot":1,"table":1,"name":""}"#.to_string(),
        Target::Response => {
            format!(r#"{{"result":"table","identity":{TABLE},"spec":{{"name":"","columns":[]}}}}"#)
        }
    };
    let empty = target.wrap(&with_name);
    let overhead = empty.len();
    let exact = empty.replace(
        "\"name\":\"\"",
        &format!("\"name\":\"{}\"", "a".repeat(MAX_BODY_BYTES - overhead)),
    );
    cases.push(case(
        "maximum_encoded_body",
        "serialization",
        raw(exact.as_bytes()),
        Expect::Accept(1),
    ));
    let escaped = empty.replace(
        "\"name\":\"\"",
        &format!(
            "\"name\":\"{}\"",
            "\\u0000".repeat((MAX_BODY_BYTES - overhead) / 6 + 1)
        ),
    );
    let mut escaped_case = case(
        "escaped_encoding_overflow",
        "serialization",
        raw(escaped.as_bytes()),
        Expect::Reject(0),
    );
    escaped_case.encoder_reject = true;
    cases.push(escaped_case);
    if matches!(target, Target::Response) {
        cases.push(case(
            "unknown_product_code",
            "product_code",
            raw(target
                .wrap(r#"{"result":"product_error","code":"XADB-ENGINE-0010"}"#)
                .as_bytes()),
            Expect::Reject(0),
        ));
    }
    cases
}

fn mutate(
    target: Target,
    seeds: &[(String, Vec<u8>)],
    random: &mut Random,
    operator: usize,
) -> Case {
    let (name, seed) = &seeds[random.index(seeds.len())];
    let body = std::str::from_utf8(&seed[4..]).unwrap();
    let tag = format!("\"{}\":", target.tag());
    let nested = format!("\"{}\":{{", target.field());
    let mut bytes = seed.clone();
    let mut expect = Expect::Differential;
    let family = match operator {
        0 => {
            let value = format!("seed:{}:\0\n한글\\\"", random.next() as i64);
            let text = body
                .replace(
                    "\"request\":1,",
                    &format!("\"request\":{},", 1 + random.index(1024)),
                )
                .replace("\"actor\":0,", &format!("\"actor\":{},", random.index(32)))
                // Match complete string values, never the performance input's
                // numeric field named `seed`. This operator promises valid JSON.
                .replace(
                    ":\"seed\"",
                    &format!(":{}", serde_json::to_string(&value).unwrap()),
                );
            bytes = raw(text.as_bytes());
            expect = Expect::Accept(1);
            "valid"
        }
        1 => {
            let at = random.index(bytes.len());
            bytes[at] ^= 1 << random.index(8);
            "differential"
        }
        2 => {
            let at = random.index(bytes.len() + 1);
            bytes.insert(at, random.next() as u8);
            "differential"
        }
        3 => {
            let at = random.index(bytes.len());
            bytes.remove(at);
            "differential"
        }
        4 => {
            let lengths = [
                0,
                1,
                MAX_BODY_BYTES as u32,
                MAX_BODY_BYTES as u32 + 1,
                u32::MAX,
            ];
            bytes[..4].copy_from_slice(&lengths[random.index(lengths.len())].to_be_bytes());
            expect = Expect::Reject(0);
            "length"
        }
        5 => {
            let end = 1 + random.index(bytes.len() - 1);
            bytes.truncate(end);
            expect = Expect::Reject(0);
            "truncation"
        }
        6 => {
            bytes.extend_from_slice(&seeds[random.index(seeds.len())].1);
            expect = Expect::Accept(2);
            "concatenation"
        }
        7 => {
            bytes = raw(body.replacen('{', "{\"unknown\":0,", 1).as_bytes());
            expect = Expect::Reject(0);
            "unknown"
        }
        8 => {
            bytes = raw(body
                .replace("\"version\":1", "\"version\":1,\"version\":1")
                .as_bytes());
            expect = Expect::Reject(0);
            "duplicate"
        }
        9 => {
            bytes = raw(body
                .replace(&format!("{tag}\""), &format!("{tag}\"unknown_"))
                .as_bytes());
            expect = Expect::Reject(0);
            "tag"
        }
        10 => {
            bytes = raw(body
                .replace(
                    "\"request\":1,",
                    &format!("\"request\":{},", [0, 1025, u64::MAX][random.index(3)]),
                )
                .as_bytes());
            expect = Expect::Reject(0);
            "identity"
        }
        11 => {
            let mut invalid = body.as_bytes().to_vec();
            invalid[1] = 0xff;
            bytes = raw(&invalid);
            expect = Expect::Reject(0);
            "utf8"
        }
        12 => {
            bytes = raw(body
                .replace(&nested, &format!("{nested}\"unknown\":null,"))
                .as_bytes());
            expect = Expect::Reject(0);
            "unknown"
        }
        13 => {
            let start = body.find(&tag).unwrap() + tag.len();
            let end = start + 1 + body[start + 1..].find('"').unwrap() + 1;
            let field = &body[start..end];
            bytes = raw(body
                .replace(&tag, &format!("{tag}{field},{tag}"))
                .as_bytes());
            expect = Expect::Reject(0);
            "duplicate"
        }
        14 => {
            bytes = raw(format!("{body} null").as_bytes());
            expect = Expect::Reject(0);
            "json"
        }
        15 => {
            bytes.extend_from_slice(&seed[..1 + random.index(3)]);
            expect = Expect::Reject(1);
            "concatenation"
        }
        16 => {
            bytes.extend_from_slice(&[0, 0, 0, 0]);
            expect = Expect::Reject(1);
            "concatenation"
        }
        _ => unreachable!(),
    };
    case(
        format!("{name}:{}", OPERATORS[operator]),
        family,
        bytes,
        expect,
    )
}

// This scanner does not call production length/decode/validate. It provides
// offsets and checks the literal framing and identity rules of accepted frames.
fn independent_frame(bytes: &[u8], start: usize) -> (usize, Value) {
    assert!(bytes.len() - start >= 4);
    let header = &bytes[start..start + 4];
    let count = header
        .iter()
        .fold(0usize, |value, byte| value * 256 + usize::from(*byte));
    assert!((1..=4 * 1024 * 1024 - 4).contains(&count));
    let end = start + 4 + count;
    assert!(end <= bytes.len());
    let body = &bytes[start + 4..end];
    std::str::from_utf8(body).unwrap();
    let value: Value = serde_json::from_slice(body).unwrap();
    assert_eq!(value["version"].as_u64(), Some(1));
    let run = value["run"].as_array().unwrap();
    assert_eq!(run.len(), 16);
    assert!(
        run.iter()
            .all(|byte| byte.as_u64().is_some_and(|byte| byte <= 255))
    );
    assert!(run.iter().any(|byte| byte.as_u64() != Some(0)));
    assert!((1..=1024).contains(&value["request"].as_u64().unwrap()));
    assert!(value["actor"].as_u64().unwrap() < 32);
    if value["observation"]["result"] == "product_error" {
        assert!(
            (1..=9).any(|code| value["observation"]["code"] == format!("XADB-ENGINE-{code:04}"))
        );
    }
    (end, value)
}

fn check<T: Envelope + Debug + PartialEq>(input: &Case, random: &mut Random) -> bool {
    assert!(input.bytes.len() <= INPUT_CAP);
    if input.encoder_reject {
        let decoded: T = serde_json::from_slice(&input.bytes[4..]).unwrap();
        assert!(frame::encode(&decoded).is_err());
    }
    let mut reader = Cursor::new(&input.bytes);
    let mut messages = Vec::new();
    let mut boundaries = Vec::new();
    let complete = loop {
        let start = reader.position() as usize;
        match frame::read::<T>(&mut reader) {
            Ok(Some(message)) => {
                let (end, _) = independent_frame(&input.bytes, start);
                assert_eq!(reader.position() as usize, end);
                let encoded = frame::encode(&message).unwrap();
                assert!((5..=MAX_FRAME_BYTES).contains(&encoded.len()));
                boundaries.push(end);
                messages.push(message);
            }
            Ok(None) => {
                assert_eq!(start, input.bytes.len());
                break true;
            }
            Err(_) => {
                assert!((reader.position() as usize) <= input.bytes.len());
                break false;
            }
        }
        assert!(messages.len() <= input.bytes.len() / 5);
    };
    match input.expect {
        Expect::Accept(count) => {
            assert!(complete);
            assert_eq!(messages.len(), count);
        }
        Expect::Reject(count) => {
            assert!(!complete);
            assert_eq!(messages.len(), count);
        }
        Expect::Differential => {}
    }
    let mut decoder = Decoder::<T>::new();
    assert_eq!(decoder.feed(&[]).unwrap(), (0, None));
    let mut offset = 0;
    let mut index = 0;
    let mut failed = false;
    let mut calls = 0;
    while offset < input.bytes.len() {
        let sizes = [1, 2, 3, 4, 7, 31, 257, 4096, 65536];
        let end = (offset + sizes[random.index(sizes.len())]).min(input.bytes.len());
        while offset < end {
            calls += 1;
            assert!(calls <= input.bytes.len() + 1, "non-progressing decoder");
            match decoder.feed(&input.bytes[offset..end]) {
                Ok((consumed, message)) => {
                    assert!(consumed > 0 && consumed <= end - offset);
                    offset += consumed;
                    if let Some(message) = message {
                        assert_eq!(Some(&message), messages.get(index));
                        assert_eq!(Some(&offset), boundaries.get(index));
                        index += 1;
                    }
                }
                Err(_) => {
                    failed = true;
                    break;
                }
            }
        }
        if failed {
            break;
        }
        assert_eq!(decoder.feed(&[]).unwrap(), (0, None));
    }
    let finished = decoder.finish().is_ok();
    assert_eq!(!failed && finished, complete);
    assert_eq!(index, messages.len());
    // Both successful EOF and every failed feed/EOF permanently close reuse.
    assert!(decoder.feed(&[]).is_err());
    assert!(decoder.feed(&[0, 0, 0, 1, b'0']).is_err());
    assert!(decoder.finish().is_err());
    complete
}

fn checked<T: Envelope + Debug + PartialEq>(
    target: Target,
    input: &Case,
    iteration: u64,
    random: &mut Random,
) -> bool {
    let chunk_state = random.0;
    match catch_unwind(AssertUnwindSafe(|| check::<T>(input, random))) {
        Ok(complete) => complete,
        Err(failure) => {
            let receipt = json!({"target":target.name(),"generator":GENERATOR,"seed":target.seed(),"iteration":iteration,"chunk_state":chunk_state,"label":input.label,"family":input.family,"expect":input.expect.json(),"encoder_reject":input.encoder_reject});
            if let Some(directory) = std::env::var_os("XADBTEST_V3_FAILURE_DIR") {
                let directory = PathBuf::from(directory);
                let base = directory.join(format!("{}-{iteration}", target.name()));
                let saved = fs::create_dir_all(&directory)
                    .and_then(|()| fs::write(base.with_extension("input"), &input.bytes))
                    .and_then(|()| {
                        fs::write(
                            base.with_extension("json"),
                            serde_json::to_vec_pretty(&receipt).unwrap(),
                        )
                    });
                if let Err(error) = saved {
                    eprintln!(
                        "failure preservation failed: {error}; receipt={receipt}; bytes={:02x?}",
                        input.bytes
                    );
                } else {
                    eprintln!(
                        "retained failure receipt: {}",
                        base.with_extension("json").display()
                    );
                }
            } else {
                eprintln!(
                    "fuzz reproduction receipt={receipt}; bytes={:02x?}",
                    input.bytes
                );
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
    corpus_cases: u64,
    variants: BTreeMap<String, u64>,
    families: BTreeMap<String, u64>,
    operators: BTreeMap<String, u64>,
}
impl Counts {
    fn run<T: Envelope + Debug + PartialEq>(
        &mut self,
        target: Target,
        input: &Case,
        random: &mut Random,
    ) {
        let accepted = checked::<T>(target, input, self.cases, random);
        self.cases = self
            .cases
            .checked_add(1)
            .expect("iteration counter exhausted before duration");
        if accepted {
            self.accepted += 1;
        } else {
            self.rejected += 1;
        }
        match input.expect {
            Expect::Accept(_) => self.known_valid += 1,
            Expect::Reject(_) => self.known_invalid += 1,
            Expect::Differential => {}
        }
        *self.families.entry(input.family.into()).or_default() += 1;
    }
}

fn replay<T: Envelope + Debug + PartialEq>(target: Target) -> bool {
    let Some(path) = std::env::var_os("XADBTEST_V3_REPLAY_RECEIPT") else {
        return false;
    };
    let path = PathBuf::from(path);
    let receipt: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(receipt["target"], target.name());
    assert_eq!(receipt["generator"], GENERATOR);
    assert_eq!(receipt["seed"].as_u64(), Some(target.seed()));
    let count = receipt["expect"]["prefix_messages"].as_u64().unwrap_or(0) as usize;
    let expect = match receipt["expect"]["kind"].as_str().unwrap() {
        "accept" => Expect::Accept(count),
        "reject" => Expect::Reject(count),
        "differential" => Expect::Differential,
        _ => panic!("invalid reproduction expectation"),
    };
    let mut input = case(
        receipt["label"].as_str().unwrap(),
        "replay",
        fs::read(path.with_extension("input")).unwrap(),
        expect,
    );
    input.encoder_reject = receipt["encoder_reject"].as_bool().unwrap();
    checked::<T>(
        target,
        &input,
        receipt["iteration"].as_u64().unwrap(),
        &mut Random(receipt["chunk_state"].as_u64().unwrap()),
    );
    true
}

fn run<T: Envelope + Debug + PartialEq>(target: Target, long: bool) {
    if !long && replay::<T>(target) {
        return;
    }
    let minimum = if long { MIN_SECONDS } else { 0 };
    println!(
        "XADB_V3_PROFILE {}",
        json!({"target":target.name(),"generator":GENERATOR,"seed":target.seed(),"min_seconds":minimum,"input_cap":INPUT_CAP,"coverage_guided":false})
    );
    let started = Instant::now();
    let seeds = corpus(target);
    let mut random = Random(target.seed());
    let mut counts = Counts::default();
    for (name, bytes) in &seeds {
        counts.run::<T>(
            target,
            &case(name, "valid", bytes.clone(), Expect::Accept(1)),
            &mut random,
        );
        *counts.variants.entry(name.clone()).or_default() += 1;
        counts.corpus_cases += 1;
    }
    for input in strict_cases(target, &seeds) {
        counts.run::<T>(target, &input, &mut random);
        counts.corpus_cases += 1;
    }
    let mut iterations = 0u64;
    while iterations < 1024 || (long && started.elapsed() < Duration::from_secs(MIN_SECONDS)) {
        let operator = random.index(OPERATORS.len());
        let input = mutate(target, &seeds, &mut random, operator);
        counts.run::<T>(target, &input, &mut random);
        *counts
            .operators
            .entry(OPERATORS[operator].into())
            .or_default() += 1;
        iterations = iterations
            .checked_add(1)
            .expect("mutation counter exhausted before duration");
    }
    let elapsed = started.elapsed();
    assert!(
        counts.accepted > 0
            && counts.rejected > 0
            && counts.known_valid > 0
            && counts.known_invalid > 0
    );
    assert_eq!(counts.variants.len(), seeds.len());
    for operator in OPERATORS {
        assert!(
            counts
                .operators
                .get(operator)
                .is_some_and(|count| *count > 0)
        );
    }
    for family in [
        "valid",
        "eof",
        "length",
        "identity",
        "duplicate",
        "unknown",
        "utf8",
        "json",
        "tag",
        "truncation",
        "concatenation",
        "serialization",
        "differential",
    ] {
        assert!(
            counts.families.get(family).is_some_and(|count| *count > 0),
            "missing family {family}"
        );
    }
    if matches!(target, Target::Response) {
        assert!(counts.families["product_code"] > 0);
    }
    assert!(elapsed >= Duration::from_secs(minimum));
    println!(
        "XADB_V3_RESULT {}",
        json!({"target":target.name(),"generator":GENERATOR,"seed":target.seed(),"elapsed_ms":elapsed.as_millis(),"cases":counts.cases,"accepted":counts.accepted,"rejected":counts.rejected,"known_valid":counts.known_valid,"known_invalid":counts.known_invalid,"corpus_cases":counts.corpus_cases,"mutations":iterations,"variant_visits":counts.variants,"family_visits":counts.families,"operator_visits":counts.operators})
    );
}

#[test]
fn request_mutation_corpus_smoke() {
    run::<Request>(Target::Request, false);
}
#[test]
fn response_mutation_corpus_smoke() {
    run::<Response>(Target::Response, false);
}
#[test]
#[ignore = "explicit V3 Request mutation fuzz: fixed minimum 1800 seconds"]
fn v3_request_parser_mutation_fuzz_30_minutes() {
    run::<Request>(Target::Request, true);
}
#[test]
#[ignore = "explicit V3 Response mutation fuzz: fixed minimum 1800 seconds"]
fn v3_response_parser_mutation_fuzz_30_minutes() {
    run::<Response>(Target::Response, true);
}
