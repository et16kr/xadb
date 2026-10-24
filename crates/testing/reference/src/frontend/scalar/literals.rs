//! Hand-authored values, errors and finite character alphabets.
use super::recipes::{spell_binary, spell_text};
use super::{
    Call, Outcome, Payload, ScalarCase, TypeTag, bad_decode, case, decode, error, outcome,
};

/// Exact scalar spellings and finite text/hex alphabets, with literal outputs.
pub fn decode_literals() -> Vec<ScalarCase> {
    let mut cases = Vec::new();
    for (id, raw, expected) in [
        ("integer-zero", "0", 0),
        ("integer-plus-zero", "+0", 0),
        ("integer-minus-zero", "-0", 0),
        ("integer-leading-zero", "0000007", 7),
        ("integer-plus-trivia", "+ /*?;*/ 42", 42),
        ("integer-minus-comment", "- --?;\n42", -42),
        ("integer-small-positive", "127", 127),
        ("integer-small-negative", "-128", -128),
        (
            "integer-max-minus-one",
            "9223372036854775806",
            9_223_372_036_854_775_806,
        ),
        ("integer-max", "9223372036854775807", i64::MAX),
        (
            "integer-explicit-plus-max",
            "+9223372036854775807",
            i64::MAX,
        ),
        (
            "integer-min-plus-one",
            "-9223372036854775807",
            -9_223_372_036_854_775_807,
        ),
        ("integer-min", "-9223372036854775808", i64::MIN),
        (
            "integer-leading-zero-max",
            "0009223372036854775807",
            i64::MAX,
        ),
        (
            "integer-leading-zero-min",
            "-0009223372036854775808",
            i64::MIN,
        ),
    ] {
        cases.push(decode(
            id,
            raw.as_bytes(),
            outcome(TypeTag::Int64, Payload::Int64(expected)),
        ));
    }
    for (id, raw) in [
        ("integer-max-plus-one", "9223372036854775808"),
        ("integer-plus-max-plus-one", "+9223372036854775808"),
        ("integer-min-minus-one", "-9223372036854775809"),
        ("integer-u64-max", "18446744073709551615"),
        ("integer-u64-overflow", "18446744073709551616"),
        ("integer-negative-u64-max", "-18446744073709551615"),
        ("integer-leading-zero-overflow", "0009223372036854775808"),
    ] {
        cases.push(bad_decode(id, raw.as_bytes(), "Range"));
    }
    for (id, raw, bytes) in [
        ("text-single-quote", "''''", "'"),
        ("text-quoted-a", "'''a'''", "'a'"),
        ("text-adjacent-quotes", "''''''", "''"),
        ("text-backslash", "'\\n'", "\\n"),
        ("text-space", "' '", " "),
        ("text-leading-trailing-space", "' a  '", " a  "),
        ("text-nul", "'\0'", "\0"),
        ("text-utf8-2", "'é'", "é"),
        ("text-utf8-3", "'가'", "가"),
        ("text-utf8-4", "'🙂'", "🙂"),
        ("text-decomposed", "'e\u{301}'", "e\u{301}"),
        ("text-control-c1", "'\u{80}\u{9f}'", "\u{80}\u{9f}"),
        ("text-del", "'\u{7f}'", "\u{7f}"),
        ("text-comments-markers", "'/*?;--*/'", "/*?;--*/"),
    ] {
        cases.push(decode(
            id,
            raw.as_bytes(),
            outcome(TypeTag::Utf8, Payload::Utf8(bytes.as_bytes().to_vec())),
        ));
    }
    cases.push(decode(
        "text-empty-typed-null",
        b"''",
        outcome(TypeTag::Utf8, Payload::Null),
    ));
    for byte in 0_u8..=127 {
        let raw = spell_text(&[byte]);
        cases.push(decode(
            format!("text-ascii-{byte:02x}"),
            &raw,
            outcome(TypeTag::Utf8, Payload::Utf8(vec![byte])),
        ));
    }
    for (id, raw, bytes) in [
        ("binary-empty-upper", b"X''".as_slice(), b"".as_slice()),
        ("binary-empty-lower", b"x''".as_slice(), b"".as_slice()),
        ("binary-zero", b"X'00'".as_slice(), b"\0".as_slice()),
        ("binary-ff-lower", b"x'ff'".as_slice(), b"\xff".as_slice()),
        (
            "binary-mixed-case",
            b"X'0aAfF0'".as_slice(),
            b"\x0a\xaf\xf0".as_slice(),
        ),
        (
            "binary-embedded-nul",
            b"X'000100'".as_slice(),
            b"\0\x01\0".as_slice(),
        ),
    ] {
        cases.push(decode(
            id,
            raw,
            outcome(TypeTag::Binary, Payload::Binary(bytes.to_vec())),
        ));
    }
    let all_bytes: Vec<u8> = (0..=255).collect();
    for uppercase in [false, true] {
        let raw = spell_binary(&all_bytes, uppercase);
        cases.push(decode(
            format!("binary-all-bytes-{uppercase}"),
            &raw,
            outcome(TypeTag::Binary, Payload::Binary(all_bytes.clone())),
        ));
    }
    for (id, raw) in [
        ("binary-odd-one", "X'0'"),
        ("binary-odd-three", "X'abc'"),
        ("binary-nonhex-first", "X'G0'"),
        ("binary-nonhex-last", "X'0G'"),
        ("binary-invalid-and-odd", "X'G'"),
        ("binary-escaped-quote", "X'0''0'"),
        ("binary-unicode-2", "X'é0'"),
        ("binary-unicode-3", "X'가0'"),
        ("binary-unicode-4", "X'🙂0'"),
        ("binary-internal-space", "X'00 00'"),
    ] {
        cases.push(bad_decode(id, raw.as_bytes(), "Syntax"));
    }
    // This explicit finite alphabet is input data, not a production hex decoder.
    for byte in 0_u8..=127 {
        if b"0123456789abcdefABCDEF".contains(&byte) {
            continue;
        }
        for position in 0..2 {
            let body = if position == 0 {
                [byte, b'0']
            } else {
                [b'0', byte]
            };
            let mut raw = vec![b'X'];
            raw.extend(spell_text(&body));
            cases.push(bad_decode(
                format!("binary-invalid-ascii-{byte:02x}-{position}"),
                &raw,
                "Syntax",
            ));
        }
    }
    cases.push(decode(
        "bool-true",
        b"TRUE",
        outcome(TypeTag::Bool, Payload::Bool(true)),
    ));
    cases.push(decode(
        "bool-false",
        b"false",
        outcome(TypeTag::Bool, Payload::Bool(false)),
    ));
    cases.push(decode("raw-null", b"NULL", Outcome::UntypedNull));
    cases.push(decode("parameter-one", b"?", Outcome::Parameter(1)));
    let mut source = b"INSERT INTO t(a) VALUES (".to_vec();
    for index in 0..256 {
        if index != 0 {
            source.push(b',');
        }
        source.push(b'?');
    }
    source.extend_from_slice(b");");
    cases.push(case(
        "parameter-256",
        Call::Decode { source, node: 262 },
        Outcome::Parameter(256),
    ));
    cases.push(case(
        "parameter-after-text",
        Call::Decode {
            source: b"INSERT INTO t(a) VALUES ('?', ?)".to_vec(),
            node: 8,
        },
        Outcome::Parameter(1),
    ));
    cases
}

/// Every non-scalar role in the simple INSERT and invalid local indices.
pub fn wrong_node_literals() -> Vec<ScalarCase> {
    let source = b"INSERT INTO t(a) VALUES (0);".to_vec();
    let mut cases = Vec::new();
    for (index, span) in [
        (0, [0, 27]),
        (1, [12, 13]),
        (2, [12, 13]),
        (3, [13, 16]),
        (4, [14, 15]),
        (5, [17, 27]),
        (6, [24, 27]),
        (8, [28, 28]),
        (usize::MAX, [28, 28]),
    ] {
        cases.push(case(
            format!("wrong-node-{index}"),
            Call::Decode {
                source: source.clone(),
                node: index,
            },
            error("Syntax", span),
        ));
    }
    cases.push(case(
        "wrong-transaction-root",
        Call::Decode {
            source: b" BEGIN; ".to_vec(),
            node: 0,
        },
        error("Syntax", [1, 6]),
    ));
    cases
}
