//! Fixed predicate syntax decisions, raw values, errors and checker corruptions.

use super::recipes::{Expr, Value, assemble, atom, eq, group, named_eq, not, one};
use super::{Case, Comparison, Kind, KnownBad, ScalarObservation as Scalar, bad, good, name, node};

fn precedence_literal() -> Case {
    good(
        "shape-hand-precedence",
        b"a=1 OR b=2 AND NOT c IS NULL",
        vec![
            node(Kind::Predicate, [0, 28], Some(1), None),
            node(Kind::Or, [0, 28], Some(2), None),
            node(Kind::And, [0, 3], Some(3), Some(6)),
            node(Kind::Compare(Comparison::Eq), [0, 3], Some(4), None),
            name(b"A", false, [0, 1], Some(5)),
            node(
                Kind::Scalar(Scalar::Integer {
                    sign: "None",
                    digits: [2, 3],
                }),
                [2, 3],
                None,
                None,
            ),
            node(Kind::And, [7, 28], Some(7), None),
            node(Kind::Compare(Comparison::Eq), [7, 10], Some(8), Some(10)),
            name(b"B", false, [7, 8], Some(9)),
            node(
                Kind::Scalar(Scalar::Integer {
                    sign: "None",
                    digits: [9, 10],
                }),
                [9, 10],
                None,
                None,
            ),
            node(Kind::Not, [15, 28], Some(11), None),
            node(Kind::IsNull { negated: false }, [19, 28], Some(12), None),
            name(b"C", false, [19, 20], None),
        ],
        0,
    )
}

fn group_literal() -> Case {
    good(
        "shape-hand-group",
        b" (a=1) ; ",
        vec![
            node(Kind::Predicate, [1, 6], Some(1), None),
            node(Kind::Or, [1, 6], Some(2), None),
            node(Kind::And, [1, 6], Some(3), None),
            node(Kind::Group, [1, 6], Some(4), None),
            node(Kind::Or, [2, 5], Some(5), None),
            node(Kind::And, [2, 5], Some(6), None),
            node(Kind::Compare(Comparison::Eq), [2, 5], Some(7), None),
            name(b"A", false, [2, 3], Some(8)),
            node(
                Kind::Scalar(Scalar::Integer {
                    sign: "None",
                    digits: [4, 5],
                }),
                [4, 5],
                None,
                None,
            ),
        ],
        0,
    )
}

/// Full arrays for precedence, operator roles, grouping and exact source spans.
pub fn literal_shapes() -> Vec<Case> {
    let mut out = vec![precedence_literal(), group_literal()];
    for (id, op, spelling) in [
        ("eq", Comparison::Eq, b"=".as_slice()),
        ("ne", Comparison::Ne, b"<>".as_slice()),
        ("lt", Comparison::Lt, b"<".as_slice()),
        ("le", Comparison::Le, b"<=".as_slice()),
        ("gt", Comparison::Gt, b">".as_slice()),
        ("ge", Comparison::Ge, b">=".as_slice()),
    ] {
        out.push(one(
            format!("shape-compare-{id}"),
            Expr::Compare(Value::name(b"a", b"A", false), op, spelling, Value::zero()),
        ));
    }
    out.push(one(
        "shape-is-null",
        Expr::IsNull(Value::name(b"a", b"A", false), false, b" IS NULL"),
    ));
    out.push(one(
        "shape-is-not-null",
        Expr::IsNull(Value::name(b"a", b"A", false), true, b" IS NOT NULL"),
    ));
    let a = named_eq(b"a", b"A", b'1');
    let b = named_eq(b"b", b"B", b'2');
    let c = named_eq(b"c", b"C", b'3');
    let d = named_eq(b"d", b"D", b'4');
    let patterns = vec![
        ("and-or", vec![vec![a.clone(), b.clone()], vec![c.clone()]]),
        ("or-and", vec![vec![a.clone()], vec![b.clone(), c.clone()]]),
        (
            "not-leading",
            vec![vec![not(a.clone()), b.clone()], vec![c.clone()]],
        ),
        (
            "not-middle",
            vec![vec![a.clone(), not(b.clone()), c.clone()]],
        ),
        (
            "not-trailing",
            vec![vec![a.clone()], vec![b.clone(), not(c.clone())]],
        ),
        ("not-double", vec![vec![not(not(a.clone()))]]),
        ("not-triple", vec![vec![not(not(not(a.clone())))]]),
        (
            "group-left-or",
            vec![vec![
                group(vec![vec![a.clone()], vec![b.clone()]]),
                c.clone(),
            ]],
        ),
        (
            "group-right-and",
            vec![
                vec![a.clone()],
                vec![group(vec![vec![b.clone(), c.clone()]])],
            ],
        ),
        (
            "not-group-and",
            vec![vec![not(group(vec![vec![a.clone(), b.clone()]]))]],
        ),
        ("group-singleton", vec![vec![group(vec![vec![a.clone()]])]]),
        (
            "group-double",
            vec![vec![group(vec![vec![group(vec![vec![a.clone()]])]])]],
        ),
        ("group-not", vec![vec![group(vec![vec![not(a.clone())]])]]),
        (
            "not-group-not",
            vec![vec![not(group(vec![vec![not(a.clone())]]))]],
        ),
        (
            "flat-and-four",
            vec![vec![a.clone(), b.clone(), c.clone(), d.clone()]],
        ),
        (
            "flat-or-four",
            vec![
                vec![a.clone()],
                vec![b.clone()],
                vec![c.clone()],
                vec![d.clone()],
            ],
        ),
        (
            "two-and-blocks",
            vec![vec![a.clone(), b.clone()], vec![c.clone(), d.clone()]],
        ),
        (
            "duplicates-and",
            vec![vec![a.clone(), a.clone(), a.clone()]],
        ),
        (
            "duplicates-or",
            vec![vec![a.clone()], vec![a.clone()], vec![a.clone()]],
        ),
        (
            "sibling-groups",
            vec![vec![
                group(vec![vec![a.clone()], vec![b.clone()]]),
                group(vec![vec![c.clone()], vec![d.clone()]]),
            ]],
        ),
    ];
    for (id, terms) in patterns {
        out.push(assemble(format!("shape-{id}"), terms, b"", b""));
    }
    out.push(one(
        "shape-not-is-not-null",
        not(Expr::IsNull(
            Value::name(b"a", b"A", false),
            true,
            b" IS NOT NULL",
        )),
    ));
    out.push(one(
        "shape-mixed-keyword-case",
        Expr::Not(
            b"nOt ",
            Box::new(Expr::IsNull(
                Value::name(b"a", b"A", false),
                true,
                b" iS nOt nUlL",
            )),
        ),
    ));
    out.push(assemble(
        "shape-trivia-crlf",
        vec![vec![a.clone()]],
        b"\t/*?;*/\r\n",
        b" ;\t-- ?;\r\n/*end*/",
    ));
    out.push(one(
        "shape-operator-trivia",
        Expr::Compare(
            Value::name(b"a", b"A", false),
            Comparison::Le,
            b" /*?;*/ <=\r\n",
            Value::integer(b"+ /*x*/ 2", "Plus", [8, 9]),
        ),
    ));
    out.push(one(
        "shape-not-comment",
        Expr::Not(b"NOT/* ? */", Box::new(a)),
    ));
    out
}

fn raw_values() -> Vec<Value> {
    vec![
        Value::integer(b"0", "None", [0, 1]),
        Value::integer(b"+ 7", "Plus", [2, 3]),
        Value::integer(b"-/*?*/2", "Minus", [6, 7]),
        Value::scalar(b"''", Scalar::Text),
        Value::scalar(b"X''", Scalar::Binary),
        Value::scalar(b"TRUE", Scalar::Bool(true)),
        Value::scalar(b"FALSE", Scalar::Bool(false)),
        Value::scalar(b"NULL", Scalar::Null),
        Value::marker(),
    ]
}

/// Raw syntax crosses types and permits unresolved markers before typed binding.
pub fn raw_value_cases() -> Vec<Case> {
    let mut out = Vec::new();
    let values = raw_values();
    for (left_index, left) in values.iter().enumerate() {
        for (right_index, right) in values.iter().enumerate() {
            out.push(one(
                format!("raw-pair-{left_index}-{right_index}"),
                eq(left.clone(), right.clone()),
            ));
        }
    }
    for (id, raw, normalized, quoted) in [
        ("lower", b"abc".as_slice(), b"ABC".as_slice(), false),
        ("mixed", b"aBc_12".as_slice(), b"ABC_12".as_slice(), false),
        ("underscore", b"_".as_slice(), b"_".as_slice(), false),
        ("quoted-lower", b"\"a\"".as_slice(), b"a".as_slice(), true),
        ("quoted-upper", b"\"A\"".as_slice(), b"A".as_slice(), true),
        (
            "quoted-quote",
            b"\"a\"\"b\"".as_slice(),
            b"a\"b".as_slice(),
            true,
        ),
        ("quoted-utf8", "\"가é\"".as_bytes(), "가é".as_bytes(), true),
        (
            "quoted-marker",
            b"\"?;\"".as_slice(),
            b"?;".as_slice(),
            true,
        ),
        (
            "quoted-keyword",
            b"\"SELECT\"".as_slice(),
            b"SELECT".as_slice(),
            true,
        ),
        ("in", b"IN".as_slice(), b"IN".as_slice(), false),
        (
            "between",
            b"between".as_slice(),
            b"BETWEEN".as_slice(),
            false,
        ),
        ("like", b"Like".as_slice(), b"LIKE".as_slice(), false),
        ("exists", b"EXISTS".as_slice(), b"EXISTS".as_slice(), false),
        (
            "default",
            b"DEFAULT".as_slice(),
            b"DEFAULT".as_slice(),
            false,
        ),
        (
            "returning",
            b"returning".as_slice(),
            b"RETURNING".as_slice(),
            false,
        ),
        ("n", b"n".as_slice(), b"N".as_slice(), false),
        ("q", b"q".as_slice(), b"Q".as_slice(), false),
        ("public", b"PUBLIC".as_slice(), b"PUBLIC".as_slice(), false),
        ("function-word", b"abs".as_slice(), b"ABS".as_slice(), false),
    ] {
        let value = Value::name(raw, normalized, quoted);
        out.push(one(
            format!("raw-name-left-{id}"),
            eq(value.clone(), Value::zero()),
        ));
        out.push(one(
            format!("raw-name-right-{id}"),
            eq(Value::zero(), value),
        ));
    }
    for (id, value) in [
        (
            "integer-positive-overflow",
            Value::integer(b"9223372036854775808", "None", [0, 19]),
        ),
        (
            "integer-negative-overflow",
            Value::integer(b"-9223372036854775809", "Minus", [1, 20]),
        ),
        (
            "integer-leading-zero",
            Value::integer(b"0000000000000000000000000000000000", "None", [0, 34]),
        ),
        ("binary-odd", Value::scalar(b"X'0'", Scalar::Binary)),
        ("binary-invalid", Value::scalar(b"x'0g'", Scalar::Binary)),
        (
            "binary-doubled-quote",
            Value::scalar(b"X'0''0'", Scalar::Binary),
        ),
        ("text-doubled-quote", Value::scalar(b"'a''b'", Scalar::Text)),
        ("text-nul", Value::scalar(b"'\0'", Scalar::Text)),
        ("text-backslash", Value::scalar(b"'a\\b'", Scalar::Text)),
        ("text-utf8", Value::scalar("'가é'".as_bytes(), Scalar::Text)),
        ("text-spaces", Value::scalar(b"'  '", Scalar::Text)),
        ("text-markers", Value::scalar(b"'?;??'", Scalar::Text)),
    ] {
        out.push(one(format!("raw-preserve-{id}"), eq(value, Value::zero())));
    }
    out.push(one(
        "raw-name-name",
        eq(
            Value::name(b"a", b"A", false),
            Value::name(b"b", b"B", false),
        ),
    ));
    out.push(one(
        "raw-marker-is-null",
        Expr::IsNull(Value::marker(), false, b" IS NULL"),
    ));
    out.push(one(
        "raw-like-is-null",
        Expr::IsNull(Value::name(b"LIKE", b"LIKE", false), false, b" IS NULL"),
    ));
    out.push(assemble(
        "raw-marker-order",
        vec![
            vec![eq(Value::marker(), Value::marker())],
            vec![not(group(vec![vec![
                Expr::IsNull(Value::marker(), false, b" IS NULL"),
                eq(Value::name(b"\"?\"", b"?", true), Value::marker()),
            ]]))],
        ],
        b"/*?*/",
        b"-- ?\n",
    ));
    out.push(assemble(
        "raw-false-prefix-bad-scalar",
        vec![vec![
            eq(Value::integer(b"1", "None", [0, 1]), Value::zero()),
            eq(
                Value::integer(b"99999999999999999999", "None", [0, 20]),
                Value::scalar(b"X'g'", Scalar::Binary),
            ),
        ]],
        b"",
        b"",
    ));
    out.push(assemble(
        "raw-true-prefix-unknown-name",
        vec![
            vec![atom()],
            vec![eq(
                Value::name(b"missing", b"MISSING", false),
                Value::scalar(b"'x'", Scalar::Text),
            )],
        ],
        b"",
        b"",
    ));
    out
}

/// Exact fixed Syntax/Unsupported/module/span expectations, without recognition.
pub fn error_cases() -> Vec<Case> {
    type ErrorLiteral<'a> = (&'a str, &'a [u8], &'a str, &'a str, [usize; 2]);
    let table: &[ErrorLiteral<'_>] = &[
        ("empty", b"", "Syntax", "Parse", [0, 0]),
        ("trivia", b" /*x*/ ", "Syntax", "Parse", [7, 7]),
        ("semicolon", b";", "Syntax", "Parse", [0, 1]),
        ("double-leading-semicolon", b";;", "Syntax", "Parse", [0, 1]),
        ("bare-name", b"a", "Syntax", "Parse", [1, 1]),
        ("bare-true", b"TRUE", "Syntax", "Parse", [4, 4]),
        ("bare-false", b"FALSE", "Syntax", "Parse", [5, 5]),
        ("bare-null", b"NULL", "Syntax", "Parse", [4, 4]),
        ("bare-marker", b"?", "Syntax", "Parse", [1, 1]),
        ("bare-text", b"'x'", "Syntax", "Parse", [3, 3]),
        ("missing-lhs", b"=1", "Syntax", "Parse", [0, 1]),
        ("missing-rhs", b"a=", "Syntax", "Parse", [2, 2]),
        ("double-equal", b"a==1", "Syntax", "Parse", [2, 3]),
        ("missing-ne-rhs", b"a<>", "Syntax", "Parse", [3, 3]),
        ("chained-comparison", b"a=b=c", "Syntax", "Parse", [3, 4]),
        ("split-ne", b"a< >1", "Syntax", "Parse", [3, 4]),
        ("split-le", b"a< =1", "Syntax", "Parse", [3, 4]),
        ("leading-and", b"AND a=1", "Syntax", "Parse", [0, 3]),
        ("leading-or", b"OR a=1", "Syntax", "Parse", [0, 2]),
        ("trailing-and", b"a=1 AND", "Syntax", "Parse", [7, 7]),
        ("trailing-or", b"a=1 OR", "Syntax", "Parse", [6, 6]),
        ("double-and", b"a=1 AND AND b=2", "Syntax", "Parse", [8, 11]),
        ("double-or", b"a=1 OR OR b=2", "Syntax", "Parse", [7, 9]),
        ("and-or-gap", b"a=1 AND OR b=2", "Syntax", "Parse", [8, 10]),
        ("bare-not", b"NOT", "Syntax", "Parse", [3, 3]),
        ("not-and", b"NOT AND a=1", "Syntax", "Parse", [4, 7]),
        ("post-not", b"a=1 NOT b=2", "Syntax", "Parse", [4, 7]),
        ("is-eof", b"a IS", "Syntax", "Parse", [4, 4]),
        ("is-not-eof", b"a IS NOT", "Syntax", "Parse", [8, 8]),
        ("is-true", b"a IS TRUE", "Unsupported", "Parse", [5, 9]),
        ("is-false", b"a IS FALSE", "Unsupported", "Parse", [5, 10]),
        (
            "is-not-true",
            b"a IS NOT TRUE",
            "Unsupported",
            "Parse",
            [9, 13],
        ),
        (
            "is-not-false",
            b"a IS NOT FALSE",
            "Unsupported",
            "Parse",
            [9, 14],
        ),
        (
            "is-distinct",
            b"a IS DISTINCT FROM b",
            "Unsupported",
            "Parse",
            [5, 13],
        ),
        (
            "is-not-distinct",
            b"a IS NOT DISTINCT FROM b",
            "Unsupported",
            "Parse",
            [9, 17],
        ),
        (
            "not-null-without-is",
            b"a NOT NULL",
            "Syntax",
            "Parse",
            [2, 5],
        ),
        (
            "is-extra-null",
            b"a IS NULL NULL",
            "Syntax",
            "Parse",
            [10, 14],
        ),
        (
            "is-double-not",
            b"a IS NOT NOT NULL",
            "Syntax",
            "Parse",
            [9, 12],
        ),
        ("is-name", b"a IS b", "Syntax", "Parse", [5, 6]),
        ("is-marker", b"a IS ?", "Syntax", "Parse", [5, 6]),
        ("group-empty", b"()", "Syntax", "Parse", [1, 2]),
        ("group-open", b"(", "Syntax", "Parse", [1, 1]),
        ("group-missing-close", b"(a=1", "Syntax", "Parse", [4, 4]),
        ("group-extra-close", b"a=1)", "Syntax", "Parse", [3, 4]),
        ("group-value", b"(a)=1", "Syntax", "Parse", [2, 3]),
        ("rhs-group-value", b"a=(1)", "Syntax", "Parse", [2, 3]),
        ("rhs-group-empty", b"a=()", "Syntax", "Parse", [2, 3]),
        (
            "group-double-missing-close",
            b"((a=1)",
            "Syntax",
            "Parse",
            [6, 6],
        ),
        ("extra-comparison", b"a=1 b=2", "Syntax", "Parse", [4, 5]),
        ("extra-semicolon", b"a=1;;", "Syntax", "Parse", [4, 5]),
        ("extra-statement", b"a=1; b=2", "Syntax", "Parse", [5, 6]),
        (
            "post-semicolon-returning",
            b"a=1; RETURNING",
            "Syntax",
            "Parse",
            [5, 14],
        ),
        (
            "post-comparison-returning",
            b"a=1 RETURNING",
            "Syntax",
            "Parse",
            [4, 13],
        ),
        (
            "post-comparison-default",
            b"a=1 DEFAULT",
            "Syntax",
            "Parse",
            [4, 11],
        ),
        ("where-entry", b"WHERE a=1", "Syntax", "Parse", [0, 5]),
        ("select-entry", b"SELECT a", "Unsupported", "Parse", [0, 6]),
        ("lex-ne", b"a!=1", "Unsupported", "Lex", [1, 3]),
        ("lex-slash", b"a/1=2", "Syntax", "Lex", [1, 2]),
        ("lex-percent", b"a%1=2", "Syntax", "Lex", [1, 2]),
        ("lex-bang", b"a!1", "Syntax", "Lex", [1, 2]),
        ("lex-nul", b"a=\0", "Syntax", "Lex", [2, 3]),
        ("lex-unclosed-text", b"a='x", "Syntax", "Lex", [2, 4]),
        ("lex-unclosed-name", b"\"a=1", "Syntax", "Lex", [0, 4]),
        ("lex-unclosed-comment", b"a=1/*x", "Syntax", "Lex", [3, 6]),
        ("lex-invalid-utf8", b"a=\xff", "Syntax", "Lex", [2, 3]),
        ("lex-truncated-utf8", b"a=\xe2\x82", "Syntax", "Lex", [2, 4]),
        ("utf8-before-parse", b"=1\xff", "Syntax", "Lex", [2, 3]),
        ("name-empty", b"\"\"=1", "Name", "Name", [0, 2]),
        ("name-nul", b"\"a\0\"=1", "Name", "Name", [0, 4]),
        ("name-del", b"\"a\x7f\"=1", "Name", "Name", [0, 4]),
        (
            "leading-dot-number",
            b".5=1",
            "Unsupported",
            "Parse",
            [0, 1],
        ),
        (
            "rhs-plus-dot-number",
            b"a=+.5",
            "Unsupported",
            "Parse",
            [3, 4],
        ),
        (
            "rhs-minus-space-dot-number",
            b"a=- .5",
            "Unsupported",
            "Parse",
            [4, 5],
        ),
        ("lhs-decimal", b"1.0=a", "Unsupported", "Parse", [1, 2]),
        ("rhs-exponent", b"a=1e+2", "Unsupported", "Parse", [3, 4]),
        (
            "rhs-exponent-digits",
            b"a=1E10",
            "Unsupported",
            "Parse",
            [3, 6],
        ),
        ("rhs-exponent-word", b"a=1efoo", "Syntax", "Parse", [3, 7]),
        ("rhs-spaced-exponent", b"a=1 e2", "Syntax", "Parse", [4, 6]),
        ("rhs-plus-marker", b"a=+?", "Syntax", "Parse", [3, 4]),
        ("rhs-minus-null", b"a=-NULL", "Syntax", "Parse", [3, 7]),
        ("rhs-double-sign", b"a=- -1", "Syntax", "Parse", [4, 5]),
        ("rhs-sign-comment", b"a=--1", "Syntax", "Parse", [5, 5]),
        ("lhs-qualified", b"t.a=1", "Unsupported", "Parse", [1, 2]),
        (
            "lhs-public-qualified",
            b"PUBLIC.a=1",
            "Unsupported",
            "Parse",
            [6, 7],
        ),
        ("rhs-qualified", b"a=t.b", "Unsupported", "Parse", [3, 4]),
        ("rhs-scalar-dot", b"a=1 . 0", "Unsupported", "Parse", [4, 5]),
        ("leading-dot-name", b".a=1", "Syntax", "Parse", [0, 1]),
        ("lhs-function", b"abs(a)=1", "Unsupported", "Parse", [3, 4]),
        (
            "lhs-quoted-function",
            b"\"abs\"(a)=1",
            "Unsupported",
            "Parse",
            [5, 6],
        ),
        (
            "lhs-space-function",
            b"f (1)=2",
            "Unsupported",
            "Parse",
            [2, 3],
        ),
        ("rhs-function", b"a=f(1)", "Unsupported", "Parse", [3, 4]),
        (
            "exists-function",
            b"EXISTS(1)",
            "Unsupported",
            "Parse",
            [6, 7],
        ),
        ("lhs-add", b"a+1=2", "Unsupported", "Parse", [1, 2]),
        ("rhs-add", b"a=1+2", "Unsupported", "Parse", [3, 4]),
        ("lhs-multiply", b"a*2=4", "Unsupported", "Parse", [1, 2]),
        ("rhs-subtract", b"a=-1-2", "Unsupported", "Parse", [4, 5]),
        (
            "lhs-select-group",
            b"(SELECT a FROM t)=1",
            "Unsupported",
            "Parse",
            [1, 7],
        ),
        (
            "rhs-select-group",
            b"a=(SELECT a FROM t)",
            "Unsupported",
            "Parse",
            [3, 9],
        ),
        ("rhs-select", b"a=SELECT", "Unsupported", "Parse", [2, 8]),
        ("operator-in", b"a IN(1)", "Unsupported", "Parse", [2, 4]),
        (
            "operator-between",
            b"a BETWEEN 1 AND 2",
            "Unsupported",
            "Parse",
            [2, 9],
        ),
        (
            "operator-like",
            b"a LIKE 'x'",
            "Unsupported",
            "Parse",
            [2, 6],
        ),
        (
            "operator-not-in",
            b"a NOT IN(1)",
            "Unsupported",
            "Parse",
            [2, 5],
        ),
        (
            "operator-not-between",
            b"a NOT BETWEEN 1 AND 2",
            "Unsupported",
            "Parse",
            [2, 5],
        ),
        (
            "operator-not-like",
            b"a NOT LIKE 'x'",
            "Unsupported",
            "Parse",
            [2, 5],
        ),
        (
            "operator-not-unknown",
            b"a NOT wat",
            "Syntax",
            "Parse",
            [2, 5],
        ),
        (
            "operator-quoted-in",
            b"a \"IN\" (1)",
            "Syntax",
            "Parse",
            [2, 6],
        ),
        (
            "operator-not-quoted-in",
            b"a NOT \"IN\" (1)",
            "Syntax",
            "Parse",
            [2, 5],
        ),
        ("prefix-n", b"N'x'=1", "Unsupported", "Parse", [0, 1]),
        ("prefix-lower-n", b"n'x'=1", "Unsupported", "Parse", [0, 1]),
        ("prefix-q", b"a=Q'x'", "Unsupported", "Parse", [2, 3]),
        ("prefix-lower-q", b"a=q'x'", "Unsupported", "Parse", [2, 3]),
        ("prefix-spaced", b"N 'x'=1", "Syntax", "Parse", [2, 5]),
        ("prefix-comment", b"Q/*x*/'x'=1", "Syntax", "Parse", [6, 9]),
        ("prefix-quoted", b"\"N\"'x'=1", "Syntax", "Parse", [3, 6]),
        ("lookahead-not-lex", b"a NOT !", "Syntax", "Lex", [6, 7]),
        ("lookahead-rhs-group-lex", b"a=(!", "Syntax", "Lex", [3, 4]),
        (
            "function-does-not-peek",
            b"f(!",
            "Unsupported",
            "Parse",
            [1, 2],
        ),
        ("rhs-group-eof", b"a=(", "Syntax", "Parse", [2, 3]),
        (
            "scalar-integer-call",
            b"1(2)=3",
            "Unsupported",
            "Parse",
            [1, 2],
        ),
        (
            "scalar-text-call",
            b"'x'(2)=3",
            "Unsupported",
            "Parse",
            [3, 4],
        ),
        (
            "scalar-null-dot",
            b"NULL.a=1",
            "Unsupported",
            "Parse",
            [4, 5],
        ),
        (
            "scalar-marker-multiply",
            b"a=?*2",
            "Unsupported",
            "Parse",
            [3, 4],
        ),
        (
            "scalar-binary-subtract",
            b"a=X''-1",
            "Unsupported",
            "Parse",
            [5, 6],
        ),
        (
            "scalar-bool-add",
            b"TRUE+1=1",
            "Unsupported",
            "Parse",
            [4, 5],
        ),
        (
            "null-test-extra-dot",
            b"a IS NULL.x",
            "Syntax",
            "Parse",
            [9, 10],
        ),
        ("rhs-extra-in", b"a=1 IN(1)", "Syntax", "Parse", [4, 6]),
        (
            "rhs-extra-not-in",
            b"a=1 NOT IN(1)",
            "Syntax",
            "Parse",
            [4, 7],
        ),
    ];
    table
        .iter()
        .map(|(id, source, code, module, span)| {
            bad(format!("error-{id}"), source, code, module, *span)
        })
        .collect()
}

pub(super) fn known_bad_observations() -> Vec<KnownBad> {
    let mut out = Vec::new();
    let mut add = |id, case: &Case, observed| {
        out.push(KnownBad {
            id,
            case: case.clone(),
            observed,
        })
    };
    let precedence = precedence_literal();
    let mut observed = precedence.expected.clone();
    observed.nodes[1].kind = Kind::And;
    add("or-to-and", &precedence, observed);
    let mut observed = precedence.expected.clone();
    observed.nodes[6].kind = Kind::Or;
    add("and-to-or", &precedence, observed);
    let mut observed = precedence.expected.clone();
    observed.nodes[10].first_child = Some(12);
    add("not-skips-null-test", &precedence, observed);
    let mut observed = precedence.expected.clone();
    observed.nodes[3].kind = Kind::Compare(Comparison::Ne);
    add("comparison-operator", &precedence, observed);
    let mut observed = precedence.expected.clone();
    observed.nodes[11].kind = Kind::IsNull { negated: true };
    add("null-test-negated", &precedence, observed);
    let mut observed = precedence.expected.clone();
    observed.nodes.push(observed.nodes[12].clone());
    add("unreachable-extra-node", &precedence, observed);
    let mut observed = precedence.expected.clone();
    observed.nodes.remove(5);
    add("missing-array-node", &precedence, observed);
    let mut observed = precedence.expected.clone();
    observed.nodes[2].next_sibling = None;
    add("missing-sibling", &precedence, observed);
    let mut observed = precedence.expected.clone();
    observed.nodes[6].next_sibling = Some(2);
    add("sibling-cycle", &precedence, observed);
    let mut observed = precedence.expected.clone();
    observed.nodes[0].first_child = Some(2);
    add("singleton-wrapper-skipped", &precedence, observed);
    let mut observed = precedence.expected.clone();
    observed.nodes.swap(4, 5);
    add("array-order", &precedence, observed);
    let mut observed = precedence.expected.clone();
    if let Kind::Name(value) = &mut observed.nodes[4].kind {
        value.bytes[0] = b'a';
    }
    add("normalized-name", &precedence, observed);
    let mut observed = precedence.expected.clone();
    if let Kind::Name(value) = &mut observed.nodes[4].kind {
        value.quoted = true;
    }
    add("quoted-name-flag", &precedence, observed);
    let mut observed = precedence.expected.clone();
    if let Kind::Name(value) = &mut observed.nodes[4].kind {
        value.span = [0, 0];
    }
    add("name-own-span", &precedence, observed);
    let mut observed = precedence.expected.clone();
    if let Kind::Scalar(Scalar::Integer { sign, .. }) = &mut observed.nodes[5].kind {
        *sign = "Plus";
    }
    add("integer-sign", &precedence, observed);
    let mut observed = precedence.expected.clone();
    if let Kind::Scalar(Scalar::Integer { digits, .. }) = &mut observed.nodes[5].kind {
        *digits = [1, 3];
    }
    add("integer-digits", &precedence, observed);
    let mut observed = precedence.expected.clone();
    observed.source[0] = b'b';
    add("source-byte", &precedence, observed);
    let mut observed = precedence.expected.clone();
    observed.complete = false;
    add("incomplete-success", &precedence, observed);
    let grouped = group_literal();
    let mut observed = grouped.expected.clone();
    observed.nodes[3].span = [2, 5];
    add("group-parentheses-span", &grouped, observed);
    let mut observed = grouped.expected.clone();
    observed.nodes[0].span = [1, 8];
    add("root-includes-semicolon", &grouped, observed);
    let mut observed = grouped.expected.clone();
    observed.source.pop();
    add("lost-trailing-trivia", &grouped, observed);
    let marker = one(
        "known-control-markers",
        eq(Value::marker(), Value::marker()),
    );
    let mut observed = marker.expected.clone();
    observed.nodes[4].kind = Kind::Scalar(Scalar::Parameter { ordinal: 0 });
    add("marker-zero", &marker, observed);
    let mut observed = marker.expected.clone();
    observed.nodes[5].kind = Kind::Scalar(Scalar::Parameter { ordinal: 1 });
    add("marker-duplicate", &marker, observed);
    let mut observed = marker.expected.clone();
    observed.nodes[4].kind = Kind::Scalar(Scalar::Parameter { ordinal: 2 });
    observed.nodes[5].kind = Kind::Scalar(Scalar::Parameter { ordinal: 1 });
    add("marker-order-swapped", &marker, observed);
    let mut observed = marker.expected.clone();
    observed.marker_count = 1;
    add("marker-count", &marker, observed);
    let text = one(
        "known-control-text-binary",
        eq(
            Value::scalar(b"''", Scalar::Text),
            Value::scalar(b"X''", Scalar::Binary),
        ),
    );
    let mut observed = text.expected.clone();
    observed.nodes[4].kind = Kind::Scalar(Scalar::Null);
    add("empty-text-to-null", &text, observed);
    let mut observed = text.expected.clone();
    observed.nodes[5].kind = Kind::Scalar(Scalar::Text);
    add("binary-to-text", &text, observed);
    let boolean = one(
        "known-control-bool",
        eq(
            Value::scalar(b"TRUE", Scalar::Bool(true)),
            Value::scalar(b"FALSE", Scalar::Bool(false)),
        ),
    );
    let mut observed = boolean.expected.clone();
    observed.nodes[4].kind = Kind::Scalar(Scalar::Bool(false));
    add("boolean-payload", &boolean, observed);
    let unicode = one(
        "known-control-utf8",
        eq(
            Value::name("\"가\"".as_bytes(), "가".as_bytes(), true),
            Value::zero(),
        ),
    );
    let mut observed = unicode.expected.clone();
    observed.nodes[4].span = [0, 3];
    add("utf8-character-span", &unicode, observed);
    let error = bad(
        "known-control-lex-error",
        b"a!=1",
        "Unsupported",
        "Lex",
        [1, 3],
    );
    let mut observed = error.expected.clone();
    observed.error.as_mut().unwrap().module = "Parse";
    add("error-module", &error, observed);
    let mut observed = error.expected.clone();
    observed.error.as_mut().unwrap().code = "Syntax";
    add("error-code", &error, observed);
    let mut observed = error.expected.clone();
    observed.error.as_mut().unwrap().span = [1, 1];
    add("error-full-to-zero-span", &error, observed);
    let mut observed = error.expected.clone();
    observed
        .nodes
        .push(node(Kind::Predicate, [0, 1], None, None));
    add("partial-tree-on-failure", &error, observed);
    let mut observed = error.expected.clone();
    observed.error = None;
    add("lost-error", &error, observed);
    let mut observed = error.expected.clone();
    observed.complete = false;
    add("incomplete-error", &error, observed);
    let eof = bad("known-control-eof-error", b"a=", "Syntax", "Parse", [2, 2]);
    let mut observed = eof.expected.clone();
    observed.error.as_mut().unwrap().span = [1, 2];
    add("eof-span", &eof, observed);
    out
}
