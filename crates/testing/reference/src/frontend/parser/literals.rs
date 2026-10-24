//! Hand-authored whole arrays and explicit offending-token fixtures.
use super::recipes::{insert, integer, quoted, raw_scalar, text, unquoted};
use super::{
    KindObservation as Kind, ParseCase, ScalarObservation as Scalar, bad, good, name, node,
};

pub(super) fn basic_insert() -> ParseCase {
    good(
        "basic-whole-array",
        b"INSERT INTO t(a) VALUES (0);",
        vec![
            node(Kind::Insert, [0, 27], Some(1), None),
            node(Kind::Target, [12, 13], Some(2), Some(3)),
            name(b"T", false, [12, 13], None),
            node(Kind::Columns, [13, 16], Some(4), Some(5)),
            name(b"A", false, [14, 15], None),
            node(Kind::Rows, [17, 27], Some(6), None),
            node(Kind::Tuple, [24, 27], Some(7), None),
            node(
                Kind::Scalar(Scalar::Integer {
                    sign: "None",
                    digits: [25, 26],
                }),
                [25, 26],
                None,
                None,
            ),
        ],
        0,
    )
}

/// Explicit arrays and source-construction cases for P2 structural semantics.
pub fn structural_literals() -> Vec<ParseCase> {
    let mut cases = vec![
        basic_insert(),
        good(
            "trivia-qualified-multirow-whole-array",
            b" /*x*/ insert into public.\"t\"(a,\"b\") values (+ /*c*/ 12,?), (NULL,''); --tail",
            vec![
                node(Kind::Insert, [7, 69], Some(1), None),
                node(Kind::Target, [19, 29], Some(2), Some(4)),
                name(b"PUBLIC", false, [19, 25], Some(3)),
                name(b"t", true, [26, 29], None),
                node(Kind::Columns, [29, 36], Some(5), Some(7)),
                name(b"A", false, [30, 31], Some(6)),
                name(b"b", true, [32, 35], None),
                node(Kind::Rows, [37, 69], Some(8), None),
                node(Kind::Tuple, [44, 58], Some(9), Some(11)),
                node(
                    Kind::Scalar(Scalar::Integer {
                        sign: "Plus",
                        digits: [53, 55],
                    }),
                    [45, 55],
                    None,
                    Some(10),
                ),
                node(
                    Kind::Scalar(Scalar::Parameter { ordinal: 1 }),
                    [56, 57],
                    None,
                    None,
                ),
                node(Kind::Tuple, [60, 69], Some(12), None),
                node(Kind::Scalar(Scalar::Null), [61, 65], None, Some(13)),
                node(Kind::Scalar(Scalar::Text), [66, 68], None, None),
            ],
            1,
        ),
        good(
            "quoted-utf8-whole-array",
            "INSERT INTO \"PUBLIC\".\"가\"(\"a\"\"b\") VALUES (X'0')".as_bytes(),
            vec![
                node(Kind::Insert, [0, 48], Some(1), None),
                node(Kind::Target, [12, 26], Some(2), Some(4)),
                name(b"PUBLIC", true, [12, 20], Some(3)),
                name("가".as_bytes(), true, [21, 26], None),
                node(Kind::Columns, [26, 34], Some(5), Some(6)),
                name(b"a\"b", true, [27, 33], None),
                node(Kind::Rows, [35, 48], Some(7), None),
                node(Kind::Tuple, [42, 48], Some(8), None),
                node(Kind::Scalar(Scalar::Binary), [43, 47], None, None),
            ],
            0,
        ),
    ];
    for (id, source, kind, span) in [
        ("begin", b"BEGIN".as_slice(), Kind::Begin, [0, 5]),
        ("commit", b"COMMIT".as_slice(), Kind::Commit, [0, 6]),
        ("rollback", b"ROLLBACK".as_slice(), Kind::Rollback, [0, 8]),
        (
            "autocommit-on",
            b"SET AUTOCOMMIT ON".as_slice(),
            Kind::SetAutocommit(true),
            [0, 17],
        ),
        (
            "autocommit-off",
            b"SET AUTOCOMMIT OFF".as_slice(),
            Kind::SetAutocommit(false),
            [0, 18],
        ),
        (
            "begin-trivia",
            b" \n/*?*/bEgIn;--x".as_slice(),
            Kind::Begin,
            [7, 12],
        ),
        (
            "commit-trivia",
            b"--x\ncommit /*?*/ ;\n".as_slice(),
            Kind::Commit,
            [4, 10],
        ),
        (
            "rollback-semicolon",
            b"rollback;".as_slice(),
            Kind::Rollback,
            [0, 8],
        ),
        (
            "set-trivia",
            b"set/*x*/autocommit\nON ; /*tail*/".as_slice(),
            Kind::SetAutocommit(true),
            [0, 21],
        ),
    ] {
        cases.push(good(id, source, vec![node(kind, span, None, None)], 0));
    }
    let zero = integer(b"0", "None", [0, 1]);
    for (id, target) in [
        ("unqualified-public", vec![unquoted(b"public", b"PUBLIC")]),
        (
            "public-uppercase",
            vec![unquoted(b"PUBLIC", b"PUBLIC"), unquoted(b"t", b"T")],
        ),
        (
            "public-mixedcase",
            vec![unquoted(b"PuBlIc", b"PUBLIC"), unquoted(b"t", b"T")],
        ),
        (
            "public-quoted",
            vec![quoted(b"\"PUBLIC\"", b"PUBLIC"), unquoted(b"t", b"T")],
        ),
        (
            "ordinary-extension-names",
            vec![unquoted(b"default", b"DEFAULT")],
        ),
        (
            "quoted-keyword-name",
            vec![quoted(b"\"SELECT\"", b"SELECT")],
        ),
        ("quoted-marker-name", vec![quoted(b"\";?\"", b";?")]),
        (
            "unicode-name-no-normalization",
            vec![quoted("\"e\u{301}\"".as_bytes(), "e\u{301}".as_bytes())],
        ),
    ] {
        cases.push(insert(
            id,
            &target,
            &[unquoted(b"returning", b"RETURNING")],
            &[vec![zero.clone()]],
        ));
    }
    cases.push(insert(
        "duplicate-columns-deferred",
        &[unquoted(b"t", b"T")],
        &[unquoted(b"a", b"A"), unquoted(b"A", b"A")],
        &[vec![zero.clone(), zero.clone()]],
    ));
    cases.push(insert(
        "arity-deferred",
        &[unquoted(b"t", b"T")],
        &[unquoted(b"a", b"A")],
        &[
            vec![zero.clone(), text(b"''")],
            vec![raw_scalar(b"NULL", Scalar::Null)],
        ],
    ));
    cases.push(insert(
        "table-column-limit-deferred",
        &[unquoted(b"t", b"T")],
        &vec![unquoted(b"a", b"A"); 33],
        &[vec![zero.clone(); 33]],
    ));
    cases.push(insert(
        "engine-mutation-limit-deferred",
        &[unquoted(b"t", b"T")],
        &[unquoted(b"a", b"A")],
        &vec![vec![zero]; 257],
    ));
    cases
}

/// Concatenate explicitly supplied before/offending/after bytes. The diagnostic
/// position is literal construction data, not a token search or error classifier.
fn at(
    id: impl Into<String>,
    before: &[u8],
    offending: &[u8],
    after: &[u8],
    code: &'static str,
    module: &'static str,
) -> ParseCase {
    let start = before.len();
    let end = start + offending.len();
    let mut source = before.to_vec();
    source.extend_from_slice(offending);
    source.extend_from_slice(after);
    bad(id, &source, code, module, [start, end])
}

/// Malformed/truncated/temporarily unsupported inputs at each P2 grammar site.
pub fn error_literals() -> Vec<ParseCase> {
    let mut cases = Vec::new();
    for (id, token, code) in [
        ("first-select", "SELECT", "Syntax"),
        ("first-update", "UPDATE", "Syntax"),
        ("first-delete", "DELETE", "Syntax"),
        ("first-create", "CREATE", "Syntax"),
        ("first-drop", "DROP", "Syntax"),
        ("first-prepare", "prepare", "Unsupported"),
        ("first-execute", "execute", "Unsupported"),
        ("first-unknown", "vacuum", "Syntax"),
        ("first-quoted-prepare", "\"PREPARE\"", "Syntax"),
        ("first-values", "VALUES", "Syntax"),
        ("first-number", "1", "Syntax"),
        ("first-marker", "?", "Syntax"),
        ("first-semicolon", ";", "Syntax"),
    ] {
        let mut case = at(id, b" /*prefix*/ ", token.as_bytes(), b" ", code, "Parse");
        if matches!(
            id,
            "first-select" | "first-update" | "first-delete" | "first-create" | "first-drop"
        ) {
            // P4 now recognizes these roots and diagnoses their missing tail.
            case.expected.error.as_mut().expect("error literal").span = [case.source.len(); 2];
        }
        cases.push(case);
    }
    for (id, source) in [
        ("empty", b"".as_slice()),
        ("trivia-only", b" \t\n--x\n/*y*/".as_slice()),
        ("set-eof", b"SET".as_slice()),
        ("set-autocommit-eof", b"SET AUTOCOMMIT".as_slice()),
        ("insert-eof", b"INSERT".as_slice()),
        ("insert-into-eof", b"INSERT INTO".as_slice()),
        ("target-eof", b"INSERT INTO t".as_slice()),
        ("prefix-dot-eof", b"INSERT INTO PUBLIC.".as_slice()),
        ("columns-open-eof", b"INSERT INTO t(".as_slice()),
        ("column-eof", b"INSERT INTO t(a".as_slice()),
        ("columns-comma-eof", b"INSERT INTO t(a,".as_slice()),
        ("columns-close-eof", b"INSERT INTO t(a)".as_slice()),
        ("values-eof", b"INSERT INTO t(a) VALUES".as_slice()),
        ("tuple-open-eof", b"INSERT INTO t(a) VALUES (".as_slice()),
        ("plus-eof", b"INSERT INTO t(a) VALUES (+".as_slice()),
        ("minus-eof", b"INSERT INTO t(a) VALUES (-".as_slice()),
        ("scalar-eof", b"INSERT INTO t(a) VALUES (0".as_slice()),
        (
            "scalar-comma-eof",
            b"INSERT INTO t(a) VALUES (0,".as_slice(),
        ),
        (
            "tuple-comma-eof",
            b"INSERT INTO t(a) VALUES (0),".as_slice(),
        ),
        (
            "comment-is-not-minus",
            b"INSERT INTO t(a) VALUES (--1".as_slice(),
        ),
    ] {
        cases.push(at(id, source, b"", b"", "Syntax", "Parse"));
    }
    for (id, before, offending, after) in [
        ("set-wrong-option", "SET ", "x", " ON"),
        ("set-equals", "SET AUTOCOMMIT ", "=", " ON"),
        ("set-bool", "SET AUTOCOMMIT ", "TRUE", ""),
        ("set-number", "SET AUTOCOMMIT ", "1", ""),
        ("insert-missing-into", "INSERT ", "t", "(a) VALUES (0)"),
        ("target-keyword", "INSERT INTO ", "SELECT", "(a) VALUES (0)"),
        ("target-number", "INSERT INTO ", "1", "(a) VALUES (0)"),
        ("target-text", "INSERT INTO ", "'t'", "(a) VALUES (0)"),
        ("target-marker", "INSERT INTO ", "?", "(a) VALUES (0)"),
        (
            "prefix-missing-name",
            "INSERT INTO PUBLIC.",
            "(",
            "a) VALUES (0)",
        ),
        (
            "prefix-keyword-name",
            "INSERT INTO PUBLIC.",
            "SELECT",
            "(a) VALUES (0)",
        ),
        (
            "three-part-target",
            "INSERT INTO PUBLIC.t",
            ".",
            "a(b) VALUES (0)",
        ),
        ("missing-columns", "INSERT INTO t ", "VALUES", " (0)"),
        ("empty-columns", "INSERT INTO t(", ")", " VALUES (0)"),
        ("column-keyword", "INSERT INTO t(", "TRUE", ") VALUES (0)"),
        ("column-marker", "INSERT INTO t(", "?", ") VALUES (0)"),
        ("column-qualified", "INSERT INTO t(a", ".", "b) VALUES (0)"),
        (
            "column-missing-comma",
            "INSERT INTO t(a ",
            "b",
            ") VALUES (0)",
        ),
        (
            "column-trailing-comma",
            "INSERT INTO t(a,",
            ")",
            " VALUES (0)",
        ),
        ("missing-values", "INSERT INTO t(a) ", "(", "0)"),
        ("as-after-columns", "INSERT INTO t(a) ", "AS", " x"),
        ("missing-tuple", "INSERT INTO t(a) VALUES ", "0", ""),
        ("empty-tuple", "INSERT INTO t(a) VALUES (", ")", ""),
        ("nested-scalar", "INSERT INTO t(a) VALUES (", "(", "0))"),
        (
            "scalar-missing-comma",
            "INSERT INTO t(a) VALUES (0 ",
            "1",
            ")",
        ),
        (
            "scalar-trailing-comma",
            "INSERT INTO t(a) VALUES (0,",
            ")",
            "",
        ),
        (
            "tuple-missing-comma",
            "INSERT INTO t(a) VALUES (0) ",
            "(",
            "1)",
        ),
        (
            "tuple-after-comma-not-open",
            "INSERT INTO t(a) VALUES (0),",
            "1",
            "",
        ),
        (
            "tuple-returning",
            "INSERT INTO t(a) VALUES (0 ",
            "RETURNING",
            ")",
        ),
        (
            "scalar-returning",
            "INSERT INTO t(a) VALUES (",
            "RETURNING",
            ")",
        ),
        ("begin-returning", "BEGIN ", "RETURNING", ""),
        ("set-returning", "SET AUTOCOMMIT OFF ", "RETURNING", ""),
        (
            "semicolon-returning",
            "INSERT INTO t(a) VALUES (0); ",
            "RETURNING",
            " a",
        ),
        ("second-semicolon", "BEGIN;", ";", ""),
        ("second-statement", "BEGIN; ", "COMMIT", ""),
        ("begin-extra-keyword", "BEGIN ", "WORK", ""),
        (
            "insert-extra-token",
            "INSERT INTO t(a) VALUES (0) ",
            "x",
            "",
        ),
        ("plus-parameter", "INSERT INTO t(a) VALUES (+", "?", ")"),
        ("minus-null", "INSERT INTO t(a) VALUES (-", "NULL", ")"),
        ("minus-minus", "INSERT INTO t(a) VALUES (- ", "-", "1)"),
        ("scalar-identifier", "INSERT INTO t(a) VALUES (", "x", ")"),
        (
            "scalar-quoted-name",
            "INSERT INTO t(a) VALUES (",
            "\"x\"",
            ")",
        ),
        ("scalar-dot", "INSERT INTO t(a) VALUES (", ".", ")"),
        (
            "numeric-space-dot",
            "INSERT INTO t(a) VALUES (1 ",
            ".",
            "5)",
        ),
        (
            "numeric-space-exponent",
            "INSERT INTO t(a) VALUES (1 ",
            "e2",
            ")",
        ),
        (
            "numeric-wrong-suffix",
            "INSERT INTO t(a) VALUES (1",
            "efoo",
            ")",
        ),
        (
            "numeric-suffix-underscore",
            "INSERT INTO t(a) VALUES (1",
            "e_2",
            ")",
        ),
        ("national-space", "INSERT INTO t(a) VALUES (", "N", " 'x')"),
        (
            "quoted-national",
            "INSERT INTO t(a) VALUES (",
            "\"N\"",
            "'x')",
        ),
        (
            "scalar-binary-space",
            "INSERT INTO t(a) VALUES (",
            "X",
            " '00')",
        ),
    ] {
        cases.push(at(
            id,
            before.as_bytes(),
            offending.as_bytes(),
            after.as_bytes(),
            "Syntax",
            "Parse",
        ));
    }
    for (id, before, offending, after) in [
        ("schema-other", "INSERT INTO ", "other", ".t(a) VALUES (0)"),
        (
            "schema-quoted-lower",
            "INSERT INTO ",
            "\"public\"",
            ".t(a) VALUES (0)",
        ),
        ("target-select", "INSERT INTO t ", "SELECT", " 1"),
        ("target-default", "INSERT INTO t ", "default", " VALUES"),
        ("target-as", "INSERT INTO t ", "AS", " x(a) VALUES (0)"),
        ("columns-select", "INSERT INTO t(a) ", "SELECT", " 1"),
        ("columns-default", "INSERT INTO t(a) ", "DEFAULT", " VALUES"),
        (
            "scalar-default",
            "INSERT INTO t(a) VALUES (",
            "default",
            ")",
        ),
        (
            "scalar-select",
            "INSERT INTO t(a) VALUES (",
            "SELECT",
            " 1)",
        ),
        (
            "insert-returning",
            "INSERT INTO t(a) VALUES (0) ",
            "returning",
            " a",
        ),
        ("fraction-dot", "INSERT INTO t(a) VALUES (", ".", "5)"),
        ("plus-fraction-dot", "INSERT INTO t(a) VALUES (+", ".", "5)"),
        (
            "minus-trivia-fraction-dot",
            "INSERT INTO t(a) VALUES (- /*x*/ ",
            ".",
            "5)",
        ),
        ("integer-dot", "INSERT INTO t(a) VALUES (1", ".", "0)"),
        (
            "integer-trailing-dot",
            "INSERT INTO t(a) VALUES (1",
            ".",
            ")",
        ),
        ("integer-exponent", "INSERT INTO t(a) VALUES (1", "e2", ")"),
        (
            "integer-exponent-uppercase",
            "INSERT INTO t(a) VALUES (1",
            "E02",
            ")",
        ),
        (
            "integer-exponent-missing",
            "INSERT INTO t(a) VALUES (1",
            "e",
            ")",
        ),
        (
            "integer-exponent-sign",
            "INSERT INTO t(a) VALUES (1",
            "e",
            "+2)",
        ),
        (
            "integer-exponent-minus",
            "INSERT INTO t(a) VALUES (-1",
            "E",
            "-2)",
        ),
        ("national-text", "INSERT INTO t(a) VALUES (", "N", "'x')"),
        ("national-lower", "INSERT INTO t(a) VALUES (", "n", "'x')"),
        (
            "alternative-quote",
            "INSERT INTO t(a) VALUES (",
            "Q",
            "'[x]')",
        ),
        (
            "alternative-lower",
            "INSERT INTO t(a) VALUES (",
            "q",
            "'[x]')",
        ),
    ] {
        cases.push(at(
            id,
            before.as_bytes(),
            offending.as_bytes(),
            after.as_bytes(),
            "Unsupported",
            "Parse",
        ));
    }
    for (id, before, offending, after) in [
        (
            "empty-target",
            b"INSERT INTO ".as_slice(),
            b"\"\"".as_slice(),
            b"(a) VALUES (0)".as_slice(),
        ),
        (
            "empty-column",
            b"INSERT INTO t(".as_slice(),
            b"\"\"".as_slice(),
            b") VALUES (0)".as_slice(),
        ),
        (
            "control-target",
            b"INSERT INTO ".as_slice(),
            b"\"a\0\"".as_slice(),
            b"(a) VALUES (0)".as_slice(),
        ),
        (
            "control-prefix",
            b"INSERT INTO ".as_slice(),
            b"\"a\x7f\"".as_slice(),
            b".t(a) VALUES (0)".as_slice(),
        ),
        (
            "control-column",
            b"INSERT INTO t(".as_slice(),
            "\"a\u{80}\"".as_bytes(),
            b") VALUES (0)".as_slice(),
        ),
    ] {
        cases.push(at(id, before, offending, after, "Name", "Name"));
    }
    for (id, before, offending, after, code) in [
        (
            "lexer-not-equal",
            b"INSERT INTO t(a) VALUES (0 ".as_slice(),
            b"!=".as_slice(),
            b" 1)".as_slice(),
            "Unsupported",
        ),
        (
            "lexer-percent",
            b"INSERT INTO t(a) VALUES (".as_slice(),
            b"%".as_slice(),
            b")".as_slice(),
            "Syntax",
        ),
        (
            "lexer-unquoted-utf8",
            b"INSERT INTO ".as_slice(),
            "가".as_bytes(),
            b"(a) VALUES (0)".as_slice(),
            "Syntax",
        ),
        (
            "lexer-unclosed-text",
            b"INSERT INTO t(a) VALUES (".as_slice(),
            b"'x".as_slice(),
            b"".as_slice(),
            "Syntax",
        ),
        (
            "lexer-unclosed-name",
            b"INSERT INTO ".as_slice(),
            b"\"t".as_slice(),
            b"".as_slice(),
            "Syntax",
        ),
        (
            "lexer-unclosed-binary",
            b"INSERT INTO t(a) VALUES (".as_slice(),
            b"X'0".as_slice(),
            b"".as_slice(),
            "Syntax",
        ),
        (
            "lexer-unclosed-comment",
            b"BEGIN ".as_slice(),
            b"/*x".as_slice(),
            b"".as_slice(),
            "Syntax",
        ),
        (
            "utf8-before-parse",
            b"wrong ".as_slice(),
            b"\xff".as_slice(),
            b"".as_slice(),
            "Syntax",
        ),
        (
            "utf8-after-valid-statement",
            b"BEGIN; --".as_slice(),
            b"\xff".as_slice(),
            b"".as_slice(),
            "Syntax",
        ),
        (
            "utf8-truncated",
            b"BEGIN; --".as_slice(),
            b"\xe2\x82".as_slice(),
            b"".as_slice(),
            "Syntax",
        ),
    ] {
        cases.push(at(id, before, offending, after, code, "Lex"));
    }
    cases
}
