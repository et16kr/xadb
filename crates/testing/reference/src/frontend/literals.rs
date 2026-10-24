//! Hand-authored lexical/name expectations; no production semantic dependencies.
//!
//! Repetition and offset translation below construct bounded test inputs. They do
//! not tokenize, decode, normalize, or inspect a production result for an answer.

/// One flat token observation, including the terminal EOF token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenObservation {
    /// Stable fixture label; the SQL test adapter maps production variants to it.
    pub kind: &'static str,
    /// Inclusive original byte offset.
    pub start: usize,
    /// Exclusive original byte offset.
    pub end: usize,
}

/// A small diagnostic observation independent of SQL's types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticObservation {
    /// Error category, without any transaction outcome.
    pub code: &'static str,
    /// Detecting module.
    pub module: &'static str,
    /// Inclusive original byte offset.
    pub start: usize,
    /// Exclusive original byte offset.
    pub end: usize,
}

/// Complete token sequence or prefix followed by one terminal diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LexObservation {
    /// Tokens in original order, with EOF only on success.
    pub tokens: Vec<TokenObservation>,
    /// The terminal failure, if any.
    pub diagnostic: Option<DiagnosticObservation>,
    /// False denotes an incomplete runner observation, never success.
    pub complete: bool,
}

/// Fixed-size-name result represented as ordinary observation bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameObservation {
    /// Expected normalized bytes, never calculated with production code.
    pub bytes: Vec<u8>,
    /// Inclusive original token offset.
    pub start: usize,
    /// Exclusive original token offset.
    pub end: usize,
    /// Whether SQL quoted-name syntax supplied the name.
    pub quoted: bool,
}

/// A lexical input and its exact independently supplied output.
#[derive(Clone, Debug)]
pub struct LexCase {
    /// Stable family-local identity.
    pub id: String,
    /// Original bytes, including invalid UTF-8 fixtures.
    pub source: Vec<u8>,
    /// Exact complete observation.
    pub expected: LexObservation,
}

/// A name input with an explicit token; malformed tokens exercise revalidation.
#[derive(Clone, Debug)]
pub struct NameCase {
    /// Stable family-local identity.
    pub id: String,
    /// Original source bytes.
    pub source: Vec<u8>,
    /// Explicit token input, not a token computed by the production scanner.
    pub token: TokenObservation,
    /// Exact bytes or name diagnostic.
    pub expected: Result<NameObservation, DiagnosticObservation>,
}

/// All 44 grammar keywords; PUBLIC and future-feature words are absent by design.
pub const KEYWORDS: &[(&str, &str)] = &[
    ("CREATE", "Keyword(Create)"),
    ("TABLE", "Keyword(Table)"),
    ("PRIMARY", "Keyword(Primary)"),
    ("KEY", "Keyword(Key)"),
    ("UNIQUE", "Keyword(Unique)"),
    ("STORAGE", "Keyword(Storage)"),
    ("DISK", "Keyword(Disk)"),
    ("BIGINT", "Keyword(Bigint)"),
    ("BOOLEAN", "Keyword(Boolean)"),
    ("VARCHAR", "Keyword(Varchar)"),
    ("VARBINARY", "Keyword(Varbinary)"),
    ("INDEX", "Keyword(Index)"),
    ("ON", "Keyword(On)"),
    ("DROP", "Keyword(Drop)"),
    ("INSERT", "Keyword(Insert)"),
    ("INTO", "Keyword(Into)"),
    ("VALUES", "Keyword(Values)"),
    ("UPDATE", "Keyword(Update)"),
    ("SET", "Keyword(Set)"),
    ("DELETE", "Keyword(Delete)"),
    ("FROM", "Keyword(From)"),
    ("SELECT", "Keyword(Select)"),
    ("AS", "Keyword(As)"),
    ("WHERE", "Keyword(Where)"),
    ("ORDER", "Keyword(Order)"),
    ("BY", "Keyword(By)"),
    ("ASC", "Keyword(Asc)"),
    ("DESC", "Keyword(Desc)"),
    ("NULLS", "Keyword(Nulls)"),
    ("FIRST", "Keyword(First)"),
    ("LAST", "Keyword(Last)"),
    ("LIMIT", "Keyword(Limit)"),
    ("NOT", "Keyword(Not)"),
    ("AND", "Keyword(And)"),
    ("OR", "Keyword(Or)"),
    ("IS", "Keyword(Is)"),
    ("NULL", "Keyword(Null)"),
    ("TRUE", "Keyword(True)"),
    ("FALSE", "Keyword(False)"),
    ("BEGIN", "Keyword(Begin)"),
    ("COMMIT", "Keyword(Commit)"),
    ("ROLLBACK", "Keyword(Rollback)"),
    ("AUTOCOMMIT", "Keyword(Autocommit)"),
    ("OFF", "Keyword(Off)"),
];

/// Construct a literal token record without applying SQL semantics.
pub fn token(kind: &'static str, start: usize, end: usize) -> TokenObservation {
    TokenObservation { kind, start, end }
}

/// Construct a literal diagnostic record without applying SQL semantics.
pub fn diagnostic(
    code: &'static str,
    module: &'static str,
    start: usize,
    end: usize,
) -> DiagnosticObservation {
    DiagnosticObservation {
        code,
        module,
        start,
        end,
    }
}

fn good(
    id: impl Into<String>,
    source: impl Into<Vec<u8>>,
    fields: &[(&'static str, usize, usize)],
) -> LexCase {
    let source = source.into();
    let mut tokens: Vec<_> = fields
        .iter()
        .map(|&(kind, start, end)| token(kind, start, end))
        .collect();
    tokens.push(token("Eof", source.len(), source.len()));
    LexCase {
        id: id.into(),
        source,
        expected: LexObservation {
            tokens,
            diagnostic: None,
            complete: true,
        },
    }
}

fn bad(
    id: impl Into<String>,
    source: impl Into<Vec<u8>>,
    code: &'static str,
    start: usize,
    end: usize,
) -> LexCase {
    LexCase {
        id: id.into(),
        source: source.into(),
        expected: LexObservation {
            tokens: Vec::new(),
            diagnostic: Some(diagnostic(code, "Lex", start, end)),
            complete: true,
        },
    }
}

/// Grammar keyword, raw-token, trivia, malformed and UTF-8 literal inventory.
pub fn lexer_literals() -> Vec<LexCase> {
    let mut cases = Vec::new();
    for &(word, kind) in KEYWORDS {
        for (style, spelling) in [
            ("upper", word.as_bytes().to_vec()),
            ("lower", word.to_ascii_lowercase().into_bytes()),
            (
                "mixed",
                word.bytes()
                    .enumerate()
                    .map(|(i, b)| {
                        if i % 2 == 0 {
                            b
                        } else {
                            b.to_ascii_lowercase()
                        }
                    })
                    .collect(),
            ),
        ] {
            cases.push(good(
                format!("keyword-{word}-{style}"),
                spelling,
                &[(kind, 0, word.len())],
            ));
        }
        cases.push(good(
            format!("keyword-{word}-suffix"),
            format!("{word}_").into_bytes(),
            &[("Identifier", 0, word.len() + 1)],
        ));
    }
    for word in [
        "PUBLIC",
        "public",
        "DEFAULT",
        "RETURNING",
        "PREPARE",
        "EXECUTE",
        "IF",
        "EXISTS",
        "CASCADE",
        "_",
        "_a9",
        "a0",
    ] {
        cases.push(good(
            format!("identifier-{word}"),
            word.as_bytes(),
            &[("Identifier", 0, word.len())],
        ));
    }
    for (id, source, kind) in [
        ("plus", "+", "Plus"),
        ("minus", "-", "Minus"),
        ("equal", "=", "Equal"),
        ("not-equal", "<>", "NotEqual"),
        ("less", "<", "Less"),
        ("less-equal", "<=", "LessEqual"),
        ("greater", ">", "Greater"),
        ("greater-equal", ">=", "GreaterEqual"),
        ("left-paren", "(", "LeftParen"),
        ("right-paren", ")", "RightParen"),
        ("comma", ",", "Comma"),
        ("dot", ".", "Dot"),
        ("star", "*", "Star"),
        ("semicolon", ";", "Semicolon"),
        ("marker", "?", "Marker"),
        ("digits", "0123", "Digits"),
        ("quoted-multibyte", "\"가\"", "QuotedIdentifier"),
        ("quoted-escape", "\"가\"\"x\"", "QuotedIdentifier"),
        ("text-escape-markers", "'a'';?'", "Text"),
        ("text-empty", "''", "Text"),
        ("text-backslash", "'a\\b'", "Text"),
        ("text-nul", "'\0'", "Text"),
        ("binary-upper", "X'0aFF'", "Binary"),
        ("binary-lower", "x'0a'", "Binary"),
        ("binary-empty", "X''", "Binary"),
        ("binary-deferred-hex", "X'0g1'", "Binary"),
        ("binary-deferred-escape", "X'0''a'", "Binary"),
    ] {
        cases.push(good(id, source.as_bytes(), &[(kind, 0, source.len())]));
    }
    for (id, source, fields) in [
        ("empty", b"".as_slice(), vec![]),
        ("ascii-whitespace", b" \t\n\x0b\x0c\r".as_slice(), vec![]),
        ("line-eof", b"--?;".as_slice(), vec![]),
        ("line-lf", b"--?;\n?".as_slice(), vec![("Marker", 5, 6)]),
        ("line-cr", b"--?;\r?".as_slice(), vec![("Marker", 5, 6)]),
        ("line-crlf", b"--?;\r\n?".as_slice(), vec![("Marker", 6, 7)]),
        (
            "block-markers",
            b"/*?;*/?".as_slice(),
            vec![("Marker", 6, 7)],
        ),
        (
            "block-not-nested",
            b"/*a/*b*/?".as_slice(),
            vec![("Marker", 8, 9)],
        ),
        (
            "comments-nul",
            b"/*\0*/--\0\n?".as_slice(),
            vec![("Marker", 9, 10)],
        ),
        (
            "name-identity",
            b"t T \"T\" \"t\"".as_slice(),
            vec![
                ("Identifier", 0, 1),
                ("Identifier", 2, 3),
                ("QuotedIdentifier", 4, 7),
                ("QuotedIdentifier", 8, 11),
            ],
        ),
        (
            "separate-binary-prefix",
            b"X '0a'".as_slice(),
            vec![("Identifier", 0, 1), ("Text", 2, 6)],
        ),
        (
            "signed-int-min",
            b"-9223372036854775808".as_slice(),
            vec![("Minus", 0, 1), ("Digits", 1, 20)],
        ),
        (
            "deferred-decimal",
            b"1.2".as_slice(),
            vec![("Digits", 0, 1), ("Dot", 1, 2), ("Digits", 2, 3)],
        ),
        (
            "deferred-exponent",
            b"1e+2".as_slice(),
            vec![
                ("Digits", 0, 1),
                ("Identifier", 1, 2),
                ("Plus", 2, 3),
                ("Digits", 3, 4),
            ],
        ),
        (
            "adjacent-operators",
            b"<><=>=".as_slice(),
            vec![
                ("NotEqual", 0, 2),
                ("LessEqual", 2, 4),
                ("GreaterEqual", 4, 6),
            ],
        ),
        (
            "multiple-statements-raw",
            b"BEGIN;COMMIT;".as_slice(),
            vec![
                ("Keyword(Begin)", 0, 5),
                ("Semicolon", 5, 6),
                ("Keyword(Commit)", 6, 12),
                ("Semicolon", 12, 13),
            ],
        ),
    ] {
        cases.push(good(id, source, &fields));
    }
    for (id, source, code, start, end) in [
        (
            "unsupported-not-equal",
            b"!=".as_slice(),
            "Unsupported",
            0,
            2,
        ),
        ("bare-bang", b"!".as_slice(), "Syntax", 0, 1),
        ("bare-slash", b"/".as_slice(), "Syntax", 0, 1),
        ("bare-percent", b"%".as_slice(), "Syntax", 0, 1),
        ("bare-dollar", b"$".as_slice(), "Syntax", 0, 1),
        ("bare-hash", b"#".as_slice(), "Syntax", 0, 1),
        ("bare-colon", b":".as_slice(), "Syntax", 0, 1),
        ("bare-backtick", b"`".as_slice(), "Syntax", 0, 1),
        ("bare-backslash", b"\\".as_slice(), "Syntax", 0, 1),
        ("outside-nul", b"\0".as_slice(), "Syntax", 0, 1),
        ("outside-del", b"\x7f".as_slice(), "Syntax", 0, 1),
        ("non-ascii-name", "가".as_bytes(), "Syntax", 0, 3),
        ("non-ascii-space", "\u{a0}".as_bytes(), "Syntax", 0, 2),
        ("unterminated-text", b"'x".as_slice(), "Syntax", 0, 2),
        ("unterminated-name", b"\"x".as_slice(), "Syntax", 0, 2),
        ("unterminated-block", b"/*x".as_slice(), "Syntax", 0, 3),
        (
            "unterminated-text-escape",
            b"'''".as_slice(),
            "Syntax",
            0,
            3,
        ),
        (
            "unterminated-name-escape",
            b"\"\"\"".as_slice(),
            "Syntax",
            0,
            3,
        ),
        ("unterminated-binary", b"X'0".as_slice(), "Syntax", 0, 3),
        ("invalid-continuation", b"\x80".as_slice(), "Syntax", 0, 1),
        ("invalid-overlong", b"\xc0\x80".as_slice(), "Syntax", 0, 1),
        (
            "invalid-surrogate",
            b"\xed\xa0\x80".as_slice(),
            "Syntax",
            0,
            1,
        ),
        (
            "invalid-too-large",
            b"\xf4\x90\x80\x80".as_slice(),
            "Syntax",
            0,
            1,
        ),
        ("invalid-first", b"a\xc2b\xff".as_slice(), "Syntax", 1, 2),
        ("invalid-in-comment", b"--\xff".as_slice(), "Syntax", 2, 3),
        ("utf8-before-unclosed", b"'\xff".as_slice(), "Syntax", 1, 2),
        ("truncated-two", b"\xc2".as_slice(), "Syntax", 0, 1),
        ("truncated-three", b"\xe2\x82".as_slice(), "Syntax", 0, 2),
        ("truncated-four", b"\xf0\x9f\x92".as_slice(), "Syntax", 0, 3),
    ] {
        cases.push(bad(id, source, code, start, end));
    }
    for (width, scalar) in [(2, "é"), (3, "가"), (4, "🙂")] {
        for offset in [255, 256, 257] {
            let mut source = vec![b' '; offset - 1];
            source.push(b'\'');
            source.extend_from_slice(scalar.as_bytes());
            source.push(b'\'');
            cases.push(good(
                format!("utf8-width-{width}-at-{offset}"),
                source,
                &[("Text", offset - 1, offset + width + 1)],
            ));
            let mut truncated = vec![b' '; offset];
            truncated.extend_from_slice(&scalar.as_bytes()[..width - 1]);
            cases.push(bad(
                format!("utf8-truncated-{width}-at-{offset}"),
                truncated,
                "Syntax",
                offset,
                offset + width - 1,
            ));
            let mut invalid = vec![b' '; offset];
            invalid.push(scalar.as_bytes()[0]);
            invalid.push(b'A');
            cases.push(bad(
                format!("utf8-invalid-{width}-at-{offset}"),
                invalid,
                "Syntax",
                offset,
                offset + 1,
            ));
        }
    }
    for (id, source, code, start, end) in [
        (
            "prefix-before-unsupported",
            b"a !=".as_slice(),
            "Unsupported",
            2,
            4,
        ),
        ("prefix-before-unclosed", b"a 'x".as_slice(), "Syntax", 2, 4),
    ] {
        let mut case = bad(id, source, code, start, end);
        case.expected.tokens.push(token("Identifier", 0, 1));
        cases.push(case);
    }
    cases
}

/// Source/token L−1, L, L+1 fixtures, with unrelated limits kept below their caps.
pub fn limit_literals() -> Vec<LexCase> {
    let mut cases = Vec::new();
    for length in [1_048_575, 1_048_576, 1_048_577] {
        let source = vec![b' '; length];
        cases.push(if length <= 1_048_576 {
            good(format!("source-{length}"), source, &[])
        } else {
            bad(
                format!("source-{length}"),
                source,
                "Limit",
                1_048_576,
                length,
            )
        });
        let mut comment = vec![b'a'; length];
        comment[..2].copy_from_slice(b"--");
        cases.push(if length <= 1_048_576 {
            good(format!("comment-source-{length}"), comment, &[])
        } else {
            bad(
                format!("comment-source-{length}"),
                comment,
                "Limit",
                1_048_576,
                length,
            )
        });
    }
    for count in [65_535, 65_536, 65_537] {
        let source = b"? ".repeat(count);
        let mut tokens: Vec<_> = (0..count.min(65_536))
            .map(|i| token("Marker", 2 * i, 2 * i + 1))
            .collect();
        let diagnostic = if count > 65_536 {
            Some(diagnostic("Limit", "Lex", 131_072, 131_072))
        } else {
            tokens.push(token("Eof", source.len(), source.len()));
            None
        };
        cases.push(LexCase {
            id: format!("token-{count}"),
            source,
            expected: LexObservation {
                tokens,
                diagnostic,
                complete: true,
            },
        });
    }
    let mut invalid = vec![b' '; 1_048_577];
    invalid[0] = 0xff;
    cases.push(bad(
        "source-before-utf8",
        invalid,
        "Limit",
        1_048_576,
        1_048_577,
    ));
    cases
}

fn name_case(id: impl Into<String>, source: Vec<u8>, quoted: bool, bytes: &[u8]) -> NameCase {
    let end = source.len();
    NameCase {
        id: id.into(),
        source,
        token: token(
            if quoted {
                "QuotedIdentifier"
            } else {
                "Identifier"
            },
            0,
            end,
        ),
        expected: Ok(NameObservation {
            bytes: bytes.to_vec(),
            start: 0,
            end,
            quoted,
        }),
    }
}

fn bad_name(id: impl Into<String>, source: Vec<u8>, input: TokenObservation) -> NameCase {
    let start = input.start.min(source.len());
    let end = input.end.min(source.len()).max(start);
    let expected = Err(diagnostic("Name", "Name", start, end));
    NameCase {
        id: id.into(),
        source,
        token: input,
        expected,
    }
}

/// Name identity, byte limits, every prohibited control, and forged token cases.
pub fn name_literals() -> Vec<NameCase> {
    let mut cases = Vec::new();
    for (id, source, quoted, bytes) in [
        ("lower", "t", false, "T"),
        ("upper", "T", false, "T"),
        ("quoted-upper", "\"T\"", true, "T"),
        ("quoted-lower", "\"t\"", true, "t"),
        ("ascii-grammar", "_a09", false, "_A09"),
        ("public-lower", "public", false, "PUBLIC"),
        ("public-upper", "PUBLIC", false, "PUBLIC"),
        ("public-quoted-upper", "\"PUBLIC\"", true, "PUBLIC"),
        ("public-quoted-lower", "\"public\"", true, "public"),
        ("multibyte", "\"가\"", true, "가"),
        ("escaped-quote", "\"가\"\"x\"", true, "가\"x"),
        ("single-quote-byte", "\"'\"", true, "'"),
        ("semicolon-marker", "\";?\"", true, ";?"),
        ("backslash", "\"a\\b\"", true, "a\\b"),
        ("space", "\" \"", true, " "),
        ("nonbreaking-space", "\"\u{a0}\"", true, "\u{a0}"),
        ("no-unicode-normalization", "\"e\u{301}\"", true, "e\u{301}"),
        ("quoted-keyword", "\"SELECT\"", true, "SELECT"),
    ] {
        cases.push(name_case(
            id,
            source.as_bytes().to_vec(),
            quoted,
            bytes.as_bytes(),
        ));
    }
    for length in [127, 128, 129] {
        for quoted in [false, true] {
            let raw = vec![b'a'; length];
            let source = if quoted {
                [b"\"".as_slice(), raw.as_slice(), b"\"".as_slice()].concat()
            } else {
                raw
            };
            let id = format!("ascii-{length}-quoted-{quoted}");
            if length <= 128 {
                cases.push(name_case(
                    id,
                    source,
                    quoted,
                    &vec![if quoted { b'a' } else { b'A' }; length],
                ));
            } else {
                let end = source.len();
                cases.push(bad_name(
                    id,
                    source,
                    token(
                        if quoted {
                            "QuotedIdentifier"
                        } else {
                            "Identifier"
                        },
                        0,
                        end,
                    ),
                ));
            }
        }
        let body = match length {
            127 => format!("{}a", "가".repeat(42)),
            128 => format!("{}aa", "가".repeat(42)),
            _ => "가".repeat(43),
        };
        let source = format!("\"{body}\"").into_bytes();
        if length <= 128 {
            cases.push(name_case(
                format!("multibyte-{length}"),
                source,
                true,
                body.as_bytes(),
            ));
        } else {
            let end = source.len();
            cases.push(bad_name(
                format!("multibyte-{length}"),
                source,
                token("QuotedIdentifier", 0, end),
            ));
        }
    }
    for length in [127, 128, 129] {
        let source = format!("\"{}\"", "\"\"".repeat(length)).into_bytes();
        if length <= 128 {
            cases.push(name_case(
                format!("escaped-length-{length}"),
                source,
                true,
                &vec![b'"'; length],
            ));
        } else {
            let end = source.len();
            cases.push(bad_name(
                format!("escaped-length-{length}"),
                source,
                token("QuotedIdentifier", 0, end),
            ));
        }
    }
    for scalar in (0..=31).chain(127..=159) {
        let source = format!(
            "\"a{}z\"",
            char::from_u32(scalar).expect("literal control scalar")
        )
        .into_bytes();
        let end = source.len();
        cases.push(bad_name(
            format!("control-{scalar:04x}"),
            source,
            token("QuotedIdentifier", 0, end),
        ));
    }
    for (id, source, kind, start, end) in [
        ("empty-unquoted", b"".as_slice(), "Identifier", 0, 0),
        ("empty-quoted", b"\"\"".as_slice(), "QuotedIdentifier", 0, 2),
        ("bad-first-digit", b"1a".as_slice(), "Identifier", 0, 2),
        ("bad-unquoted-symbol", b"a$".as_slice(), "Identifier", 0, 2),
        ("bad-unquoted-nonascii", "가".as_bytes(), "Identifier", 0, 3),
        ("missing-close", b"\"a".as_slice(), "QuotedIdentifier", 0, 2),
        (
            "unescaped-interior",
            b"\"a\"b\"".as_slice(),
            "QuotedIdentifier",
            0,
            5,
        ),
        ("wrong-kind", b"a".as_slice(), "Digits", 0, 1),
        (
            "keyword-kind",
            b"SELECT".as_slice(),
            "Keyword(Select)",
            0,
            6,
        ),
        (
            "keyword-forged-as-identifier",
            b"SELECT".as_slice(),
            "Identifier",
            0,
            6,
        ),
        ("reversed-span", b"ab".as_slice(), "Identifier", 2, 1),
        ("out-of-bounds", b"ab".as_slice(), "Identifier", 0, 3),
        ("split-utf8", "\"가\"".as_bytes(), "QuotedIdentifier", 2, 5),
        (
            "invalid-utf8",
            b"\"\xff\"".as_slice(),
            "QuotedIdentifier",
            0,
            3,
        ),
    ] {
        cases.push(bad_name(id, source.to_vec(), token(kind, start, end)));
    }
    let mut offset = name_case(
        "offset-token",
        "\"가\"".as_bytes().to_vec(),
        true,
        "가".as_bytes(),
    );
    offset.source = b"  \"\xea\xb0\x80\"  ".to_vec();
    offset.token.start = 2;
    offset.token.end = 7;
    if let Ok(value) = &mut offset.expected {
        value.start = 2;
        value.end = 7;
    }
    cases.push(offset);
    cases
}

/// Fixed V2 seeds, intentionally independent of host randomness.
pub const MERGE_SEEDS: std::ops::RangeInclusive<u64> = 0..=63;

/// Compose literal fragments; expected offsets are translated, never re-lexed.
pub fn generated_literals(seed: u64, operations: usize) -> LexCase {
    const FRAGMENTS: &[(&[u8], Option<&str>)] = &[
        (b"a", Some("Identifier")),
        (b"123", Some("Digits")),
        (b"?", Some("Marker")),
        (b"'a'';?'", Some("Text")),
        (b"\"\xea\xb0\x80\"", Some("QuotedIdentifier")),
        (b"X'0aff'", Some("Binary")),
        (b"<=", Some("LessEqual")),
        (b"--?;\r\n", None),
        (b"/*?;*/", None),
        (b"SELECT", Some("Keyword(Select)")),
        (b";", Some("Semicolon")),
        (b" ", None),
        (b"\t\x0b\x0c", None),
        (b"\"\"\"\"", Some("QuotedIdentifier")),
        (b"+", Some("Plus")),
        (b"-", Some("Minus")),
    ];
    let mut state = seed.wrapping_add(1);
    let mut source = Vec::new();
    let mut tokens = Vec::new();
    for _ in 0..operations {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let (fragment, kind) = FRAGMENTS[((state >> 32) as usize) % FRAGMENTS.len()];
        let start = source.len();
        source.extend_from_slice(fragment);
        if let Some(kind) = kind {
            tokens.push(token(kind, start, source.len()));
        }
        source.push(b' ');
    }
    tokens.push(token("Eof", source.len(), source.len()));
    LexCase {
        id: format!("generated-{seed}-{operations}"),
        source,
        expected: LexObservation {
            tokens,
            diagnostic: None,
            complete: true,
        },
    }
}
