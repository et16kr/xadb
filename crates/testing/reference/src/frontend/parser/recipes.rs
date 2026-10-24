//! Concatenation recipes whose shape, names and scalar payloads are given data.
use super::{
    KindObservation as Kind, NodeObservation, ParseCase, ScalarObservation as Scalar, bad, good,
    name, node,
};

#[derive(Clone)]
pub(super) struct NameLiteral {
    raw: Vec<u8>,
    normalized: Vec<u8>,
    quoted: bool,
}

pub(super) fn unquoted(raw: &[u8], normalized: &[u8]) -> NameLiteral {
    NameLiteral {
        raw: raw.to_vec(),
        normalized: normalized.to_vec(),
        quoted: false,
    }
}

pub(super) fn quoted(raw: &[u8], normalized: &[u8]) -> NameLiteral {
    NameLiteral {
        raw: raw.to_vec(),
        normalized: normalized.to_vec(),
        quoted: true,
    }
}

#[derive(Clone)]
pub(super) struct ScalarLiteral {
    raw: Vec<u8>,
    kind: Scalar,
}

pub(super) fn integer(raw: &[u8], sign: &'static str, digits: [usize; 2]) -> ScalarLiteral {
    ScalarLiteral {
        raw: raw.to_vec(),
        kind: Scalar::Integer { sign, digits },
    }
}

pub(super) fn text(raw: &[u8]) -> ScalarLiteral {
    raw_scalar(raw, Scalar::Text)
}
pub(super) fn raw_scalar(raw: &[u8], kind: Scalar) -> ScalarLiteral {
    ScalarLiteral {
        raw: raw.to_vec(),
        kind,
    }
}

fn append_name(source: &mut Vec<u8>, value: &NameLiteral) -> NodeObservation {
    let start = source.len();
    source.extend_from_slice(&value.raw);
    name(&value.normalized, value.quoted, [start, source.len()], None)
}

/// This is a source constructor, not a recognizer: all grammar choices and
/// semantic tags arrive explicitly. It never branches on source byte contents.
pub(super) fn insert(
    id: impl Into<String>,
    target: &[NameLiteral],
    columns: &[NameLiteral],
    rows: &[Vec<ScalarLiteral>],
) -> ParseCase {
    assert!((1..=2).contains(&target.len()) && !columns.is_empty() && !rows.is_empty());
    let mut source = b"INSERT INTO ".to_vec();
    let mut nodes = vec![
        node(Kind::Insert, [0, 0], Some(1), None),
        node(Kind::Target, [12, 0], Some(2), None),
    ];
    for (index, value) in target.iter().enumerate() {
        if index != 0 {
            source.push(b'.');
        }
        let mut value = append_name(&mut source, value);
        if index + 1 < target.len() {
            value.next_sibling = Some(nodes.len() + 1);
        }
        nodes.push(value);
    }
    nodes[1].span[1] = source.len();
    let columns_index = nodes.len();
    nodes[1].next_sibling = Some(columns_index);
    let columns_start = source.len();
    source.push(b'(');
    nodes.push(node(
        Kind::Columns,
        [columns_start, 0],
        Some(columns_index + 1),
        None,
    ));
    for (index, value) in columns.iter().enumerate() {
        if index != 0 {
            source.push(b',');
        }
        let mut value = append_name(&mut source, value);
        if index + 1 < columns.len() {
            value.next_sibling = Some(nodes.len() + 1);
        }
        nodes.push(value);
    }
    source.push(b')');
    nodes[columns_index].span[1] = source.len();
    source.push(b' ');
    let rows_index = nodes.len();
    nodes[columns_index].next_sibling = Some(rows_index);
    let rows_start = source.len();
    source.extend_from_slice(b"VALUES ");
    nodes.push(node(
        Kind::Rows,
        [rows_start, 0],
        Some(rows_index + 1),
        None,
    ));
    let mut marker_count = 0;
    for (row_index, row) in rows.iter().enumerate() {
        assert!(!row.is_empty());
        if row_index != 0 {
            source.push(b',');
        }
        let tuple_index = nodes.len();
        let tuple_start = source.len();
        source.push(b'(');
        let next_tuple = (row_index + 1 < rows.len()).then_some(tuple_index + 1 + row.len());
        nodes.push(node(
            Kind::Tuple,
            [tuple_start, 0],
            Some(tuple_index + 1),
            next_tuple,
        ));
        for (index, value) in row.iter().enumerate() {
            if index != 0 {
                source.push(b',');
            }
            let start = source.len();
            source.extend_from_slice(&value.raw);
            let mut kind = value.kind.clone();
            match &mut kind {
                Scalar::Integer { digits, .. } => {
                    digits[0] += start;
                    digits[1] += start;
                }
                Scalar::Parameter { ordinal } => {
                    marker_count += 1;
                    *ordinal = marker_count;
                }
                _ => {}
            }
            let next = (index + 1 < row.len()).then_some(nodes.len() + 1);
            nodes.push(node(Kind::Scalar(kind), [start, source.len()], None, next));
        }
        source.push(b')');
        nodes[tuple_index].span[1] = source.len();
    }
    nodes[rows_index].span[1] = source.len();
    nodes[0].span[1] = source.len();
    good(id, &source, nodes, marker_count)
}

pub(super) fn one_scalar(id: impl Into<String>, value: ScalarLiteral) -> ParseCase {
    insert(
        id,
        &[unquoted(b"t", b"T")],
        &[unquoted(b"a", b"A")],
        &[vec![value]],
    )
}

pub(super) fn marker_insert(count: usize) -> ParseCase {
    let rows = vec![vec![raw_scalar(b"?", Scalar::Parameter { ordinal: 0 })]; count];
    insert(
        format!("markers-{count}"),
        &[unquoted(b"t", b"T")],
        &[unquoted(b"a", b"A")],
        &rows,
    )
}

/// Fixed scalar spellings with independently supplied relative digits spans.
/// Integer overflow, odd/nonhex binary and empty text stay raw P2 syntax.
pub fn scalar_literals() -> Vec<ParseCase> {
    let mut cases = Vec::new();
    for (id, raw, sign, digits) in [
        ("zero", b"0".as_slice(), "None", [0, 1]),
        ("leading-zeroes", b"000001".as_slice(), "None", [0, 6]),
        ("plus", b"+12".as_slice(), "Plus", [1, 3]),
        ("minus", b"-12".as_slice(), "Minus", [1, 3]),
        ("plus-trivia", b"+ /*?;*/ 12".as_slice(), "Plus", [9, 11]),
        (
            "minus-line-comment",
            b"- --?;\n12".as_slice(),
            "Minus",
            [7, 9],
        ),
        (
            "int64-max",
            b"9223372036854775807".as_slice(),
            "None",
            [0, 19],
        ),
        (
            "int64-max-plus-one",
            b"9223372036854775808".as_slice(),
            "None",
            [0, 19],
        ),
        (
            "int64-min",
            b"-9223372036854775808".as_slice(),
            "Minus",
            [1, 20],
        ),
        (
            "int64-min-minus-one",
            b"-9223372036854775809".as_slice(),
            "Minus",
            [1, 20],
        ),
        (
            "unsigned64-overflow",
            b"18446744073709551616".as_slice(),
            "None",
            [0, 20],
        ),
    ] {
        cases.push(one_scalar(id, integer(raw, sign, digits)));
    }
    for (id, raw, kind) in [
        ("empty-text", b"''".as_slice(), Scalar::Text),
        ("escaped-text", b"'a''b'".as_slice(), Scalar::Text),
        ("text-backslash", b"'a\\b'".as_slice(), Scalar::Text),
        ("text-nul", b"'\0'".as_slice(), Scalar::Text),
        ("text-utf8", "'가🙂'".as_bytes(), Scalar::Text),
        ("text-punctuation", b"'?;/*--*/'".as_slice(), Scalar::Text),
        ("empty-binary", b"X''".as_slice(), Scalar::Binary),
        ("binary-hex", b"x'00aF'".as_slice(), Scalar::Binary),
        ("binary-odd", b"X'0'".as_slice(), Scalar::Binary),
        ("binary-nonhex", b"X'GG'".as_slice(), Scalar::Binary),
        (
            "binary-escaped-quote",
            b"X'a''b'".as_slice(),
            Scalar::Binary,
        ),
        ("true", b"TRUE".as_slice(), Scalar::Bool(true)),
        ("false", b"false".as_slice(), Scalar::Bool(false)),
        ("null", b"NuLl".as_slice(), Scalar::Null),
        (
            "parameter",
            b"?".as_slice(),
            Scalar::Parameter { ordinal: 0 },
        ),
    ] {
        cases.push(one_scalar(id, raw_scalar(raw, kind)));
    }
    cases
}

/// Independently specified source, name, raw-value, marker and node boundaries.
pub fn boundary_cases() -> Vec<ParseCase> {
    let mut cases = Vec::new();
    for length in [1_048_575, 1_048_576, 1_048_577] {
        let mut source = b"BEGIN".to_vec();
        source.resize(length, b' ');
        if length <= 1_048_576 {
            cases.push(good(
                format!("source-{length}"),
                &source,
                vec![node(Kind::Begin, [0, 5], None, None)],
                0,
            ));
        } else {
            cases.push(bad(
                format!("source-{length}"),
                &source,
                "Limit",
                "Lex",
                [1_048_576, 1_048_577],
            ));
        }
    }
    for length in [127, 128, 129] {
        for quoted_name in [false, true] {
            let raw = if quoted_name {
                format!("\"{}\"", "a".repeat(length)).into_bytes()
            } else {
                vec![b'a'; length]
            };
            let value = if quoted_name {
                quoted(&raw, &vec![b'a'; length])
            } else {
                unquoted(&raw, &vec![b'A'; length])
            };
            let mut case = insert(
                format!("target-name-{length}-{quoted_name}"),
                &[value],
                &[unquoted(b"a", b"A")],
                &[vec![integer(b"0", "None", [0, 1])]],
            );
            if length > 128 {
                case = bad(case.id, &case.source, "Name", "Name", [12, 12 + raw.len()]);
            }
            cases.push(case);
        }
    }
    for count in [255, 256, 257] {
        let mut case = marker_insert(count);
        if count == 257 {
            // Prefix length25, each preceding row consumes four bytes: ?),(.
            case = bad(
                case.id,
                &case.source,
                "Limit",
                "Parse",
                [25 + 4 * 256, 25 + 4 * 256],
            );
        }
        cases.push(case);
    }
    for (count, target_count, columns_count, rows_count) in
        [(8191, 1, 2, 2728), (8192, 1, 1, 4093), (8193, 2, 1, 4093)]
    {
        let target = if target_count == 1 {
            vec![unquoted(b"t", b"T")]
        } else {
            vec![unquoted(b"PUBLIC", b"PUBLIC"), unquoted(b"t", b"T")]
        };
        let columns = vec![unquoted(b"a", b"A"); columns_count];
        let rows = vec![vec![integer(b"0", "None", [0, 1]); columns_count]; rows_count];
        let mut case = insert(format!("nodes-{count}"), &target, &columns, &rows);
        if count == 8193 {
            // Qualified prefix makes the final scalar the 8,193rd node.
            let start = 32 + 4 * 4092;
            case = bad(case.id, &case.source, "Limit", "Parse", [start, start]);
        }
        cases.push(case);
    }
    for length in [65_535, 65_536, 65_537] {
        let mut raw = vec![b'\''];
        raw.extend(std::iter::repeat_n(b'a', length));
        raw.push(b'\'');
        cases.push(one_scalar(format!("raw-text-decoded-{length}"), text(&raw)));
        let mut raw = b"X'".to_vec();
        raw.extend(std::iter::repeat_n(b'0', length * 2));
        raw.push(b'\'');
        cases.push(one_scalar(
            format!("raw-binary-decoded-{length}"),
            raw_scalar(&raw, Scalar::Binary),
        ));
    }
    cases.push(one_scalar(
        "raw-integer-65537-digits",
        integer(&vec![b'9'; 65_537], "None", [0, 65_537]),
    ));
    cases
}

/// Fixed, replayable input construction. 64×128 is required; 256×1024 is
/// separately selectable. Each run uses at most one marker per sixteen scalars.
pub fn generated_case(seed: u64, fragments: usize) -> ParseCase {
    assert!((1..=1024).contains(&fragments));
    let pool = [
        integer(b"0", "None", [0, 1]),
        integer(b"+001", "Plus", [1, 4]),
        integer(b"-9", "Minus", [1, 2]),
        integer(b"+/*?;*/42", "Plus", [7, 9]),
        integer(b"- \n 7", "Minus", [4, 5]),
        text(b"'a''b'"),
        text(b"''"),
        raw_scalar(b"X''", Scalar::Binary),
        raw_scalar(b"X'0'", Scalar::Binary),
        raw_scalar(b"x'GG'", Scalar::Binary),
        raw_scalar(b"TRUE", Scalar::Bool(true)),
        raw_scalar(b"false", Scalar::Bool(false)),
        raw_scalar(b"NULL", Scalar::Null),
        raw_scalar(b"?", Scalar::Parameter { ordinal: 0 }),
        text("'가;?'".as_bytes()),
        integer(b"9223372036854775808", "None", [0, 19]),
    ];
    let mut state = seed;
    let mut rows = Vec::new();
    for index in 0..fragments {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let choice = if index % 16 == (seed & 15) as usize {
            13
        } else {
            let choice = ((state >> 32) % 15) as usize;
            if choice >= 13 { choice + 1 } else { choice }
        };
        rows.push(vec![pool[choice].clone()]);
    }
    let target = match seed % 4 {
        0 => vec![unquoted(b"t", b"T")],
        1 => vec![unquoted(b"public", b"PUBLIC"), quoted(b"\"t\"", b"t")],
        2 => vec![
            quoted(b"\"PUBLIC\"", b"PUBLIC"),
            quoted("\"가\"".as_bytes(), "가".as_bytes()),
        ],
        _ => vec![unquoted(b"returning", b"RETURNING")],
    };
    let mut case = insert(
        format!("generated-{seed}-{fragments}"),
        &target,
        &[unquoted(b"default", b"DEFAULT")],
        &rows,
    );
    let suffix: &[u8] = match seed % 3 {
        0 => b"; -- ? tail",
        1 => b" /* ? tail */",
        _ => b"",
    };
    case.source.extend_from_slice(suffix);
    case.expected.source.extend_from_slice(suffix);
    case
}
