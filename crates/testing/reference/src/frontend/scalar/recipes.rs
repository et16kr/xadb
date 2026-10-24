//! Expected-value-first spelling and bounded repetition, never an input decoder.
use super::{
    Origin, Outcome, Payload, ScalarCase, Spec, TypeTag, bad_decode, copy, decode, error, outcome,
    value,
};

/// Produce an SQL spelling from already-known text bytes; no output is decoded.
pub(super) fn spell_text(bytes: &[u8]) -> Vec<u8> {
    let mut raw = vec![b'\''];
    for byte in bytes {
        raw.push(*byte);
        if *byte == b'\'' {
            raw.push(b'\'');
        }
    }
    raw.push(b'\'');
    raw
}

pub(super) fn spell_binary(bytes: &[u8], uppercase: bool) -> Vec<u8> {
    let alphabet = if uppercase {
        b"0123456789ABCDEF"
    } else {
        b"0123456789abcdef"
    };
    let mut raw = b"X'".to_vec();
    for byte in bytes {
        raw.push(alphabet[usize::from(byte / 16)]);
        raw.push(alphabet[usize::from(byte % 16)]);
    }
    raw.push(b'\'');
    raw
}

fn spec(logical_type: TypeTag, maximum_bytes: Option<usize>, nullable: bool) -> Spec {
    Spec {
        logical_type,
        maximum_bytes,
        nullable,
    }
}

#[derive(Clone, Copy)]
enum Cell {
    Original,
    Type,
    NotNull,
    Utf8Null,
}
const SAME: [Cell; 2] = [Cell::Original, Cell::Original];
const TYPE: [Cell; 2] = [Cell::Type, Cell::Type];
const NULL: [Cell; 2] = [Cell::NotNull, Cell::Original];
const EMPTY_SQL: [Cell; 2] = [Cell::NotNull, Cell::Utf8Null];

/// Twelve inputs × four target types × two nullable flags × two origins.
/// The expected outcomes are explicit finite tables, not a copied validator.
pub fn copy_matrix() -> Vec<ScalarCase> {
    use TypeTag::{Binary, Bool, Int64, Utf8};
    let inputs = [
        ("int", value(Int64, Payload::Int64(7))),
        ("bool", value(Bool, Payload::Bool(true))),
        ("text", value(Utf8, Payload::Utf8(b"x".to_vec()))),
        ("binary", value(Binary, Payload::Binary(b"x".to_vec()))),
        ("null-int", value(Int64, Payload::Null)),
        ("null-bool", value(Bool, Payload::Null)),
        ("null-text", value(Utf8, Payload::Null)),
        ("null-binary", value(Binary, Payload::Null)),
        ("empty-text", value(Utf8, Payload::Utf8(Vec::new()))),
        ("empty-binary", value(Binary, Payload::Binary(Vec::new()))),
        ("space-text", value(Utf8, Payload::Utf8(b" ".to_vec()))),
        ("nul-text", value(Utf8, Payload::Utf8(b"\0".to_vec()))),
    ];
    let sql = [
        [SAME, TYPE, TYPE, TYPE],
        [TYPE, SAME, TYPE, TYPE],
        [TYPE, TYPE, SAME, TYPE],
        [TYPE, TYPE, TYPE, SAME],
        [NULL, TYPE, TYPE, TYPE],
        [TYPE, NULL, TYPE, TYPE],
        [TYPE, TYPE, NULL, TYPE],
        [TYPE, TYPE, TYPE, NULL],
        [TYPE, TYPE, EMPTY_SQL, TYPE],
        [TYPE, TYPE, TYPE, SAME],
        [TYPE, TYPE, SAME, TYPE],
        [TYPE, TYPE, SAME, TYPE],
    ];
    let native = [
        [SAME, TYPE, TYPE, TYPE],
        [TYPE, SAME, TYPE, TYPE],
        [TYPE, TYPE, SAME, TYPE],
        [TYPE, TYPE, TYPE, SAME],
        [NULL, TYPE, TYPE, TYPE],
        [TYPE, NULL, TYPE, TYPE],
        [TYPE, TYPE, NULL, TYPE],
        [TYPE, TYPE, TYPE, NULL],
        [TYPE, TYPE, SAME, TYPE],
        [TYPE, TYPE, TYPE, SAME],
        [TYPE, TYPE, SAME, TYPE],
        [TYPE, TYPE, SAME, TYPE],
    ];
    let targets = [
        spec(Int64, None, false),
        spec(Bool, None, false),
        spec(Utf8, Some(65_536), false),
        spec(Binary, Some(65_536), false),
    ];
    let mut cases = Vec::new();
    for (origin, table) in [(Origin::Sql, sql), (Origin::Native, native)] {
        for (input_index, (id, input)) in inputs.iter().enumerate() {
            for (target_index, target) in targets.iter().enumerate() {
                for (nullable_index, nullable) in [false, true].into_iter().enumerate() {
                    let mut target = *target;
                    target.nullable = nullable;
                    let expected = match table[input_index][target_index][nullable_index] {
                        Cell::Original => Outcome::Value(input.clone()),
                        Cell::Type => error("Type", [31, 37]),
                        Cell::NotNull => error("NotNull", [31, 37]),
                        Cell::Utf8Null => outcome(Utf8, Payload::Null),
                    };
                    cases.push(copy(
                        format!(
                            "matrix-{origin:?}-{id}-{:?}-{nullable}",
                            target.logical_type
                        ),
                        origin,
                        input.clone(),
                        target,
                        [31, 37],
                        expected,
                    ));
                }
            }
        }
    }
    cases
}

/// Raw decoded byte limits, long integers and explicitly ordered binary errors.
pub fn decode_boundaries() -> Vec<ScalarCase> {
    let mut cases = Vec::new();
    for length in [255, 256, 257, 65_537] {
        for negative in [false, true] {
            let mut raw = if negative { vec![b'-'] } else { Vec::new() };
            raw.extend(std::iter::repeat_n(b'0', length));
            cases.push(decode(
                format!("integer-long-zero-{length}-{negative}"),
                &raw,
                outcome(TypeTag::Int64, Payload::Int64(0)),
            ));
        }
    }
    for (id, suffix, expected) in [
        (
            "long-leading-zero-max",
            b"9223372036854775807".as_slice(),
            i64::MAX,
        ),
        (
            "long-leading-zero-min",
            b"9223372036854775808".as_slice(),
            i64::MIN,
        ),
    ] {
        let mut raw = if expected == i64::MIN {
            vec![b'-']
        } else {
            Vec::new()
        };
        raw.extend(std::iter::repeat_n(b'0', 65_537));
        raw.extend_from_slice(suffix);
        cases.push(decode(
            id,
            &raw,
            outcome(TypeTag::Int64, Payload::Int64(expected)),
        ));
    }
    cases.push(bad_decode(
        "integer-long-nine-overflow",
        &vec![b'9'; 65_537],
        "Range",
    ));
    for (length, valid) in [(65_535, true), (65_536, true), (65_537, false)] {
        for (width, fragment) in [
            (1, b"a".as_slice()),
            (2, "é".as_bytes()),
            (3, "가".as_bytes()),
            (4, "🙂".as_bytes()),
        ] {
            let mut bytes = fragment.repeat(length / width);
            bytes.extend(std::iter::repeat_n(b'a', length % width));
            let raw = spell_text(&bytes);
            let id = format!("decoded-text-{length}-width{width}");
            cases.push(if valid {
                decode(id, &raw, outcome(TypeTag::Utf8, Payload::Utf8(bytes)))
            } else {
                bad_decode(id, &raw, "Range")
            });
        }
        let bytes = vec![b'\''; length];
        let raw = spell_text(&bytes);
        let id = format!("decoded-escaped-text-{length}");
        cases.push(if valid {
            decode(id, &raw, outcome(TypeTag::Utf8, Payload::Utf8(bytes)))
        } else {
            bad_decode(id, &raw, "Range")
        });
        let bytes = vec![0xab; length];
        let raw = spell_binary(&bytes, false);
        let id = format!("decoded-binary-{length}");
        cases.push(if valid {
            decode(id, &raw, outcome(TypeTag::Binary, Payload::Binary(bytes)))
        } else {
            bad_decode(id, &raw, "Range")
        });
    }
    for (id, pairs_before, bad_suffix, code) in [
        ("binary-invalid-before-range", 0, b"GG".as_slice(), "Syntax"),
        (
            "binary-high-invalid-at-range",
            65_536,
            b"G0".as_slice(),
            "Syntax",
        ),
        (
            "binary-low-invalid-at-range",
            65_536,
            b"0G".as_slice(),
            "Syntax",
        ),
        (
            "binary-range-before-invalid",
            65_537,
            b"GG".as_slice(),
            "Range",
        ),
        (
            "binary-valid-odd-after-limit",
            65_536,
            b"0".as_slice(),
            "Syntax",
        ),
        ("binary-range-before-odd", 65_537, b"0".as_slice(), "Range"),
        (
            "binary-invalid-odd-after-limit",
            65_536,
            b"G".as_slice(),
            "Syntax",
        ),
    ] {
        let mut raw = b"X'".to_vec();
        raw.extend(std::iter::repeat_n(b'0', pairs_before * 2));
        raw.extend_from_slice(bad_suffix);
        if pairs_before == 0 {
            raw.extend(std::iter::repeat_n(b'0', 65_537 * 2));
        }
        raw.push(b'\'');
        cases.push(bad_decode(id, &raw, code));
    }
    cases
}

/// Explicit metadata, assignment-byte and source-origin boundaries.
pub fn copy_boundaries() -> Vec<ScalarCase> {
    use TypeTag::{Binary, Bool, Int64, Utf8};
    let mut cases = Vec::new();
    for origin in [Origin::Sql, Origin::Native] {
        for logical_type in [Utf8, Binary] {
            for (length, valid) in [(65_535, true), (65_536, true), (65_537, false)] {
                let payload = if logical_type == Utf8 {
                    Payload::Utf8(vec![b'a'; length])
                } else {
                    Payload::Binary(vec![0xa5; length])
                };
                let input = value(logical_type, payload);
                let expected = if valid {
                    Outcome::Value(input.clone())
                } else {
                    error("Range", [41, 47])
                };
                cases.push(copy(
                    format!("copy-global-{origin:?}-{logical_type:?}-{length}"),
                    origin,
                    input,
                    spec(logical_type, Some(65_536), false),
                    [41, 47],
                    expected,
                ));
            }
            for maximum in [1, 6] {
                for (length, status) in [
                    (maximum - 1, "valid"),
                    (maximum, "valid"),
                    (maximum + 1, "range"),
                ] {
                    let payload = if logical_type == Utf8 {
                        Payload::Utf8(vec![b'a'; length])
                    } else {
                        Payload::Binary(vec![0x80; length])
                    };
                    let input = value(logical_type, payload);
                    let expected = match (status, origin, logical_type, length) {
                        ("range", _, _, _) => error("Range", [41, 47]),
                        (_, Origin::Sql, Utf8, 0) => outcome(Utf8, Payload::Null),
                        _ => Outcome::Value(input.clone()),
                    };
                    cases.push(copy(
                        format!("copy-target-{origin:?}-{logical_type:?}-{maximum}-{length}"),
                        origin,
                        input,
                        spec(logical_type, Some(maximum), true),
                        [41, 47],
                        expected,
                    ));
                }
            }
            for (maximum, code) in [
                (0, "Range"),
                (1, "Type"),
                (65_536, "Type"),
                (65_537, "Range"),
                (usize::MAX, "Range"),
            ] {
                cases.push(copy(
                    format!("copy-metadata-{origin:?}-{logical_type:?}-{maximum}"),
                    origin,
                    value(Int64, Payload::Null),
                    spec(logical_type, Some(maximum), false),
                    [41, 47],
                    error(code, [41, 47]),
                ));
            }
        }
        for (width, fragment) in [
            (2, "é".as_bytes()),
            (3, "가".as_bytes()),
            (4, "🙂".as_bytes()),
        ] {
            for (length, valid) in [(65_535, true), (65_536, true), (65_537, false)] {
                let mut bytes = fragment.repeat(length / width);
                bytes.extend(std::iter::repeat_n(b'a', length % width));
                let input = value(Utf8, Payload::Utf8(bytes));
                let expected = if valid {
                    Outcome::Value(input.clone())
                } else {
                    error("Range", [41, 47])
                };
                cases.push(copy(
                    format!("copy-multibyte-{origin:?}-{width}-{length}"),
                    origin,
                    input,
                    spec(Utf8, Some(65_536), false),
                    [41, 47],
                    expected,
                ));
            }
        }
        for input in [
            value(Utf8, Payload::Utf8(vec![b'a'; 65_537])),
            value(Binary, Payload::Binary(vec![0; 65_537])),
        ] {
            cases.push(copy(
                format!("copy-type-before-size-{origin:?}-{:?}", input.logical_type),
                origin,
                input,
                spec(Bool, None, false),
                [41, 47],
                error("Type", [41, 47]),
            ));
        }
        for (id, bytes) in [
            ("nul", b"a\0b".as_slice()),
            ("space", b" a  ".as_slice()),
            ("backslash", b"\\n".as_slice()),
            ("decomposed", "e\u{301}".as_bytes()),
            ("composed", "é".as_bytes()),
            ("korean", "가나".as_bytes()),
            ("supplementary", "🙂".as_bytes()),
            ("controls", b"\x01\x7f".as_slice()),
        ] {
            let input = value(Utf8, Payload::Utf8(bytes.to_vec()));
            cases.push(copy(
                format!("copy-preserve-{origin:?}-{id}"),
                origin,
                input.clone(),
                spec(Utf8, Some(6), false),
                [41, 47],
                Outcome::Value(input),
            ));
        }
        let input = value(Binary, Payload::Binary((0..=255).collect()));
        cases.push(copy(
            format!("copy-all-bytes-{origin:?}"),
            origin,
            input.clone(),
            spec(Binary, Some(256), false),
            [41, 47],
            Outcome::Value(input),
        ));
    }
    for (id, origin, at) in [
        ("copy-reversed-caller-span", Origin::Sql, [99, 3]),
        (
            "copy-max-caller-span",
            Origin::Native,
            [usize::MAX - 1, usize::MAX],
        ),
    ] {
        cases.push(copy(
            id,
            origin,
            value(Utf8, Payload::Null),
            spec(Int64, None, false),
            at,
            error("Type", at),
        ));
    }
    cases
}

/// Required64 seeds×128 scalar operations, and separately selected expansion.
/// Each branch starts from a known value/error then constructs its raw spelling.
pub fn generated_cases(seed: u64, operations: usize) -> Vec<ScalarCase> {
    assert!((1..=1024).contains(&operations));
    let mut state = seed;
    let mut cases = Vec::new();
    for operation in 0..operations {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let id = format!("generated-{seed}-{operation}");
        let item = match (state >> 32) % 8 {
            0 => {
                let number = ((state & 65535) as i64) - 32768;
                decode(
                    id,
                    number.to_string().as_bytes(),
                    outcome(TypeTag::Int64, Payload::Int64(number)),
                )
            }
            1 => {
                let bytes = [b'a', (state & 127) as u8, b'\'', b' '];
                decode(
                    id,
                    &spell_text(&bytes),
                    outcome(TypeTag::Utf8, Payload::Utf8(bytes.to_vec())),
                )
            }
            2 => {
                let bytes = [(state & 255) as u8, ((state >> 8) & 255) as u8, 0, 255];
                decode(
                    id,
                    &spell_binary(&bytes, state & 1 == 0),
                    outcome(TypeTag::Binary, Payload::Binary(bytes.to_vec())),
                )
            }
            3 => decode(id, b"''", outcome(TypeTag::Utf8, Payload::Null)),
            4 => decode(id, b"NULL", Outcome::UntypedNull),
            5 => decode(id, b"?", Outcome::Parameter(1)),
            6 => bad_decode(id, b"9223372036854775808", "Range"),
            _ => {
                let input = value(TypeTag::Utf8, Payload::Utf8(Vec::new()));
                copy(
                    id,
                    Origin::Native,
                    input.clone(),
                    spec(TypeTag::Utf8, Some(1), false),
                    [5, 2],
                    Outcome::Value(input),
                )
            }
        };
        cases.push(item);
    }
    cases
}
