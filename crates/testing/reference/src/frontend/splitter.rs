//! Independent batch-boundary observations and literal fixtures for DEV4.
//!
//! This module must not import the production scanner, splitter or source owner.
//! Statement boundaries and bytes are supplied by hand-authored literal vectors;
//! partition expansion only moves those fixed boundaries between caller chunks.

/// A complete source segment returned by the batch boundary recognizer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatementObservation {
    /// Exact retained source bytes, including the contract's retained trivia.
    pub bytes: Vec<u8>,
    /// Inclusive original batch byte offset.
    pub start: usize,
    /// Exclusive original batch byte offset.
    pub end: usize,
    /// Whether an unquoted semicolon terminated this segment.
    pub terminated: bool,
}

/// One non-error result of consuming a caller-supplied chunk prefix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventObservation {
    /// The consumed prefix has no complete nonempty segment yet.
    NeedMore,
    /// One source segment is ready; later input remains the caller's responsibility.
    Statement(StatementObservation),
    /// Explicit EOF was consumed without a remaining nonempty segment.
    Eof,
}

/// Exact result of one splitter call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepObservation {
    /// Byte count consumed from this call's supplied slice.
    pub consumed: usize,
    /// Boundary event after that prefix.
    pub event: EventObservation,
}

/// A splitter-local diagnostic with original batch offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorObservation {
    /// Error category; no statement or transaction outcome is inferred.
    pub code: &'static str,
    /// Module that detected the error.
    pub module: &'static str,
    /// Inclusive original batch offset.
    pub start: usize,
    /// Exclusive original batch offset.
    pub end: usize,
}

/// Complete stream observation, including terminal errors and call completeness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceObservation {
    /// Every non-error step in order.
    pub steps: Vec<StepObservation>,
    /// Terminal diagnostic if a call failed.
    pub error: Option<ErrorObservation>,
    /// False marks an interrupted or bounded-out observation.
    pub complete: bool,
}

/// An explicit call input and its independently fixed result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallFixture {
    /// Explicit push or finish operation; a chunk boundary is never EOF.
    pub action: ActionFixture,
    /// Exact expected call result.
    pub expected: Result<StepObservation, ErrorObservation>,
}

/// One caller operation; final EOF is always a separate operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionFixture {
    /// Exact bytes, including an unconsumed suffix that may need resubmission.
    Push(Vec<u8>),
    /// The caller has reached actual EOF.
    Finish,
}

/// One independently enumerated batch input and chunk schedule.
#[derive(Clone, Debug)]
pub struct SplitCase {
    /// Stable fixture identity.
    pub id: String,
    /// Ordered calls with literal results, independent of production state.
    pub calls: Vec<CallFixture>,
}

/// Strict comparison of a complete observation to an explicit fixture trace.
pub fn check_trace(case: &SplitCase, observed: &TraceObservation) -> bool {
    if !observed.complete {
        return false;
    }
    let mut steps = Vec::new();
    let mut error = None;
    for call in &case.calls {
        match &call.expected {
            Ok(step) => steps.push(step.clone()),
            Err(value) => {
                error = Some(value.clone());
                break;
            }
        }
    }
    observed.steps == steps && observed.error == error
}

/// A hand-authored source and its statement/error boundaries before partitioning.
#[derive(Clone, Debug)]
pub struct BatchLiteral {
    /// Stable boundary-case identity.
    pub id: &'static str,
    /// Whole original batch input.
    pub source: Vec<u8>,
    /// Complete emitted segments, in order. Bytes and offsets are literal data.
    pub statements: Vec<StatementObservation>,
    /// Absolute consumed-byte count at which an invalid byte becomes detectable.
    /// None denotes an EOF-only error or no error.
    pub error_after: Option<usize>,
    /// Exact terminal error, if present.
    pub error: Option<ErrorObservation>,
}

fn statement(bytes: &[u8], start: usize, end: usize, terminated: bool) -> StatementObservation {
    StatementObservation {
        bytes: bytes.to_vec(),
        start,
        end,
        terminated,
    }
}

fn good(id: &'static str, source: &[u8], statements: Vec<StatementObservation>) -> BatchLiteral {
    BatchLiteral {
        id,
        source: source.to_vec(),
        statements,
        error_after: None,
        error: None,
    }
}

fn bad(
    id: &'static str,
    source: &[u8],
    statements: Vec<StatementObservation>,
    after: Option<usize>,
    start: usize,
    end: usize,
) -> BatchLiteral {
    BatchLiteral {
        id,
        source: source.to_vec(),
        statements,
        error_after: after,
        error: Some(ErrorObservation {
            code: "Syntax",
            module: "Split",
            start,
            end,
        }),
    }
}

/// Twelve short inputs whose every byte-boundary partition is enumerated.
/// Their 892 schedules include split quote escapes, comment delimiters and UTF-8.
pub fn partition_literals() -> Vec<BatchLiteral> {
    vec![
        good(
            "two-statements",
            b"a;b",
            vec![statement(b"a;", 0, 2, true), statement(b"b", 2, 3, false)],
        ),
        good(
            "text-semicolon",
            b"'a;';",
            vec![statement(b"'a;';", 0, 5, true)],
        ),
        good(
            "text-escape",
            b"'a'';';",
            vec![statement(b"'a'';';", 0, 7, true)],
        ),
        good(
            "name-semicolon",
            b"\";\";",
            vec![statement(b"\";\";", 0, 4, true)],
        ),
        good(
            "name-escape",
            b"\"\"\"\";",
            vec![statement(b"\"\"\"\";", 0, 5, true)],
        ),
        good(
            "block-semicolon",
            b"/*;*/a;",
            vec![statement(b"/*;*/a;", 0, 7, true)],
        ),
        good(
            "line-lf",
            b"--;\na;",
            vec![statement(b"--;\na;", 0, 6, true)],
        ),
        good(
            "utf8-three",
            "'가';".as_bytes(),
            vec![statement("'가';".as_bytes(), 0, 6, true)],
        ),
        good(
            "utf8-four",
            "'🙂';".as_bytes(),
            vec![statement("'🙂';".as_bytes(), 0, 7, true)],
        ),
        good(
            "line-crlf",
            b"--;\r\na;",
            vec![statement(b"--;\r\na;", 0, 7, true)],
        ),
        good(
            "binary-quote",
            b"X';';",
            vec![statement(b"X';';", 0, 5, true)],
        ),
        good(
            "non-nested-comment",
            b"/*a/*b*/?;",
            vec![statement(b"/*a/*b*/?;", 0, 10, true)],
        ),
    ]
}

/// Longer/EOF/malformed inputs, each exercised whole, at every one-cut and bytewise.
pub fn edge_literals() -> Vec<BatchLiteral> {
    vec![
        good(
            "pending-minus-replay",
            b"-a;",
            vec![statement(b"-a;", 0, 3, true)],
        ),
        good(
            "pending-slash-replay",
            b"/;",
            vec![statement(b"/;", 0, 2, true)],
        ),
        good(
            "block-repeated-star",
            b"/***/a;",
            vec![statement(b"/***/a;", 0, 7, true)],
        ),
        good("empty", b"", vec![]),
        good("only-whitespace", b" \t\r\n\x0b\x0c", vec![]),
        good(
            "empty-segments",
            b"; \t;/*x*/; a; --tail",
            vec![statement(b" a;", 10, 13, true)],
        ),
        good(
            "retained-trivia",
            b" a; \n b ",
            vec![
                statement(b" a;", 0, 3, true),
                statement(b" \n b ", 3, 8, false),
            ],
        ),
        good(
            "parenthesis-does-not-hide-semicolon",
            b"(a;b);",
            vec![statement(b"(a;", 0, 3, true), statement(b"b);", 3, 6, true)],
        ),
        good(
            "closed-text-eof",
            b"''",
            vec![statement(b"''", 0, 2, false)],
        ),
        good(
            "closed-name-eof",
            b"\"\"",
            vec![statement(b"\"\"", 0, 2, false)],
        ),
        good("line-comment-eof", b"--x", vec![]),
        good(
            "nonempty-line-comment-eof",
            b"a--x",
            vec![statement(b"a--x", 0, 4, false)],
        ),
        good("block-comment-eof", b"/*x*/", vec![]),
        good("minus-eof", b"-", vec![statement(b"-", 0, 1, false)]),
        good("slash-eof", b"/", vec![statement(b"/", 0, 1, false)]),
        good(
            "multiple-empty",
            b"a;;;b;",
            vec![statement(b"a;", 0, 2, true), statement(b"b;", 4, 6, true)],
        ),
        good("only-semicolons", b";;", vec![]),
        good(
            "line-cr",
            b"--;\ra;",
            vec![statement(b"--;\ra;", 0, 6, true)],
        ),
        good("text-nul", b"'\0';", vec![statement(b"'\0';", 0, 4, true)]),
        good(
            "backslash-not-escape",
            b"'\\';",
            vec![statement(b"'\\';", 0, 4, true)],
        ),
        good(
            "parenthesized-comment",
            b"(/*;*/);",
            vec![statement(b"(/*;*/);", 0, 8, true)],
        ),
        good(
            "line-terminator-before-semi",
            b"a--\n;",
            vec![statement(b"a--\n;", 0, 5, true)],
        ),
        good(
            "nul-is-parser-work",
            b"a;\0;",
            vec![statement(b"a;", 0, 2, true), statement(b"\0;", 2, 4, true)],
        ),
        good(
            "unsupported-is-parser-work",
            b"a;!=;",
            vec![statement(b"a;", 0, 2, true), statement(b"!=;", 2, 5, true)],
        ),
        good(
            "non-ascii-is-lexer-work",
            "가;".as_bytes(),
            vec![statement("가;".as_bytes(), 0, 4, true)],
        ),
        good(
            "trailing-crlf",
            b"a;\r\n",
            vec![statement(b"a;", 0, 2, true)],
        ),
        good("non-nested-empty", b"/*a/*b*/;", vec![]),
        good(
            "comment-after-token",
            b"a/*x*/;",
            vec![statement(b"a/*x*/;", 0, 7, true)],
        ),
        good(
            "pending-minus-after-token",
            b"a-",
            vec![statement(b"a-", 0, 2, false)],
        ),
        good(
            "pending-slash-after-token",
            b"a/",
            vec![statement(b"a/", 0, 2, false)],
        ),
        bad(
            "prefix-before-invalid",
            b"a;\xff",
            vec![statement(b"a;", 0, 2, true)],
            Some(3),
            2,
            3,
        ),
        bad(
            "prefix-before-bad-continuation",
            b"a;\xc2A",
            vec![statement(b"a;", 0, 2, true)],
            Some(4),
            2,
            3,
        ),
        bad(
            "prefix-before-carry-eof",
            b"a;\xe2\x82",
            vec![statement(b"a;", 0, 2, true)],
            None,
            2,
            4,
        ),
        bad("invalid-comment-byte", b"--\xff", vec![], Some(3), 2, 3),
        bad(
            "carry-before-open-comment",
            b"/*\xe2\x82",
            vec![],
            None,
            2,
            4,
        ),
        bad("open-text", b"'abc", vec![], None, 0, 4),
        bad("open-name", b"\"abc", vec![], None, 0, 4),
        bad("open-comment", b"/*abc", vec![], None, 0, 5),
        bad("binary-quote-position", b"X'abc", vec![], None, 1, 5),
        bad("text-escape-needs-close", b"'''", vec![], None, 0, 3),
        bad("name-escape-needs-close", b"\"\"\"", vec![], None, 0, 3),
        bad(
            "absolute-open-comment",
            b"a; /*abc",
            vec![statement(b"a;", 0, 2, true)],
            None,
            3,
            8,
        ),
        bad("semicolon-inside-open-text", b"';", vec![], None, 0, 2),
        bad(
            "prefix-pending-delimiter-invalid",
            b"a;/\xff",
            vec![statement(b"a;", 0, 2, true)],
            Some(4),
            3,
            4,
        ),
        bad("carry-two", b"\xc2", vec![], None, 0, 1),
        bad("carry-three", b"\xe2\x82", vec![], None, 0, 2),
        bad("carry-four", b"\xf0\x9f\x92", vec![], None, 0, 3),
        bad("stray-continuation", b"\x80", vec![], Some(1), 0, 1),
        bad("overlong", b"\xc0\x80", vec![], Some(1), 0, 1),
        bad("surrogate", b"\xed\xa0\x80", vec![], Some(2), 0, 1),
        bad("too-large", b"\xf4\x90\x80\x80", vec![], Some(2), 0, 1),
        bad("only-open-text", b"'", vec![], None, 0, 1),
        bad("only-open-name", b"\"", vec![], None, 0, 1),
        bad("only-open-comment", b"/*", vec![], None, 0, 2),
    ]
}

fn push_call(bytes: &[u8], consumed: usize, event: EventObservation) -> CallFixture {
    CallFixture {
        action: ActionFixture::Push(bytes.to_vec()),
        expected: Ok(StepObservation { consumed, event }),
    }
}

/// Translate fixed literal boundaries into calls; no byte is classified here.
/// `ends` is a strictly increasing list of chunk end offsets ending at source.len.
pub fn partition_case(
    literal: &BatchLiteral,
    ends: &[usize],
    suffix: impl std::fmt::Display,
) -> SplitCase {
    let mut calls = Vec::new();
    let mut position = 0;
    let mut output = 0;
    for &end in ends {
        assert!(position <= end && end <= literal.source.len());
        loop {
            let next = literal
                .statements
                .get(output)
                .filter(|item| item.terminated && item.end <= end);
            if let Some(item) = next {
                assert!(literal.error_after.is_none_or(|after| after > item.end));
                calls.push(push_call(
                    &literal.source[position..end],
                    item.end - position,
                    EventObservation::Statement(item.clone()),
                ));
                position = item.end;
                output += 1;
                if position == end {
                    break;
                }
                continue;
            }
            if literal.error_after.is_some_and(|after| after <= end) {
                calls.push(CallFixture {
                    action: ActionFixture::Push(literal.source[position..end].to_vec()),
                    expected: Err(literal.error.clone().expect("literal error boundary")),
                });
                return SplitCase {
                    id: format!("{}-{suffix}", literal.id),
                    calls,
                };
            }
            calls.push(push_call(
                &literal.source[position..end],
                end - position,
                EventObservation::NeedMore,
            ));
            position = end;
            break;
        }
    }
    assert_eq!(position, literal.source.len());
    let expected = if let Some(error) = &literal.error {
        Err(error.clone())
    } else if let Some(item) = literal.statements.get(output) {
        assert!(!item.terminated);
        Ok(StepObservation {
            consumed: 0,
            event: EventObservation::Statement(item.clone()),
        })
    } else {
        Ok(StepObservation {
            consumed: 0,
            event: EventObservation::Eof,
        })
    };
    let failed = expected.is_err();
    calls.push(CallFixture {
        action: ActionFixture::Finish,
        expected,
    });
    if !failed {
        calls.push(CallFixture {
            action: ActionFixture::Finish,
            expected: Ok(StepObservation {
                consumed: 0,
                event: EventObservation::Eof,
            }),
        });
    }
    SplitCase {
        id: format!("{}-{suffix}", literal.id),
        calls,
    }
}

/// Every subset of the internal byte boundaries of the short literal source.
pub fn all_partitions(literal: &BatchLiteral) -> Vec<SplitCase> {
    let length = literal.source.len();
    assert!((1..=16).contains(&length));
    (0..1usize << (length - 1))
        .map(|mask| {
            let mut ends: Vec<_> = (1..length)
                .filter(|offset| mask & (1 << (offset - 1)) != 0)
                .collect();
            ends.push(length);
            partition_case(literal, &ends, format_args!("partition-{mask}"))
        })
        .collect()
}

/// Whole input, each single split and bytewise schedule, deduplicated by offsets.
pub fn edge_partitions(literal: &BatchLiteral) -> Vec<SplitCase> {
    use std::collections::BTreeSet;
    let length = literal.source.len();
    assert!(
        length <= 256,
        "one-cut expansion is only for short edge inputs"
    );
    let mut schedules = BTreeSet::new();
    schedules.insert(vec![length]);
    for cut in 1..length {
        schedules.insert(vec![cut, length]);
    }
    if length > 0 {
        schedules.insert((1..=length).collect());
    }
    schedules
        .into_iter()
        .enumerate()
        .map(|(index, ends)| partition_case(literal, &ends, format_args!("edge-{index}")))
        .collect()
}

/// One 128-fragment deterministic stream: fragment outputs are literal, translated.
pub fn generated_literal(seed: u64, fragments: usize) -> BatchLiteral {
    let choices = partition_literals();
    let mut source = Vec::new();
    let mut statements = Vec::new();
    let mut state = seed.wrapping_add(1);
    for _ in 0..fragments {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        // Exclude the first case's unterminated tail: all generated fragments
        // end at a known external delimiter, so boundaries concatenate exactly.
        let fragment = &choices[1 + ((state >> 32) as usize) % (choices.len() - 1)];
        let offset = source.len();
        source.extend_from_slice(&fragment.source);
        for item in &fragment.statements {
            let mut shifted = item.clone();
            shifted.start += offset;
            shifted.end += offset;
            statements.push(shifted);
        }
    }
    BatchLiteral {
        id: "generated",
        source,
        statements,
        error_after: None,
        error: None,
    }
}

/// One seeded stream with deterministic 1..=17-byte chunks and 128 fixed fragments.
pub fn generated_case(seed: u64, fragments: usize) -> SplitCase {
    let literal = generated_literal(seed, fragments);
    let mut state = seed ^ 0x5841_4442_5350_4c54;
    let mut end = 0;
    let mut ends = Vec::new();
    while end < literal.source.len() {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        end = (end + 1 + ((state >> 32) as usize) % 17).min(literal.source.len());
        ends.push(end);
    }
    partition_case(&literal, &ends, format_args!("seed-{seed}-{fragments}"))
}

/// Large source limits use only fixed cuts, never O(length) separate schedules.
pub fn limit_cases() -> Vec<SplitCase> {
    let mut cases = Vec::new();
    for length in [1_048_575, 1_048_576, 1_048_577] {
        for comment in [false, true] {
            let mut source = vec![b'a'; length];
            if comment {
                source[..2].copy_from_slice(b"--");
            }
            let mut literal = good(
                if comment {
                    "comment-limit"
                } else {
                    "source-limit"
                },
                &source,
                if !comment && length <= 1_048_576 {
                    vec![statement(&source, 0, length, false)]
                } else {
                    Vec::new()
                },
            );
            if length > 1_048_576 {
                literal.error_after = Some(1_048_577);
                literal.error = Some(ErrorObservation {
                    code: "Limit",
                    module: "Split",
                    start: 1_048_576,
                    end: 1_048_577,
                });
            }
            cases.push(partition_case(
                &literal,
                &[length],
                format_args!("whole-{length}"),
            ));
            let mut cuts = vec![1, 255, 256, 257, length - 1, length];
            cuts.sort_unstable();
            cuts.dedup();
            cases.push(partition_case(
                &literal,
                &cuts,
                format_args!("fixed-cuts-{length}"),
            ));
        }
    }
    let mut source = vec![b'a'; 1_048_576];
    source[1_048_575] = b';';
    source.extend_from_slice(b"b;");
    let literal = good(
        "source-limit-resets",
        &source,
        vec![
            statement(&source[..1_048_576], 0, 1_048_576, true),
            statement(b"b;", 1_048_576, 1_048_578, true),
        ],
    );
    cases.push(partition_case(&literal, &[1_048_578], "whole"));
    cases
}

/// Explicit empty pushes and finished-state behavior independent of chunk schedules.
pub fn lifecycle_literals() -> Vec<SplitCase> {
    vec![SplitCase {
        id: "empty-push-is-not-eof".into(),
        calls: vec![
            push_call(b"", 0, EventObservation::NeedMore),
            push_call(b"a", 1, EventObservation::NeedMore),
            push_call(b"", 0, EventObservation::NeedMore),
            push_call(
                b";",
                1,
                EventObservation::Statement(statement(b"a;", 0, 2, true)),
            ),
            CallFixture {
                action: ActionFixture::Finish,
                expected: Ok(StepObservation {
                    consumed: 0,
                    event: EventObservation::Eof,
                }),
            },
            CallFixture {
                action: ActionFixture::Push(Vec::new()),
                expected: Err(ErrorObservation {
                    code: "Syntax",
                    module: "Split",
                    start: 2,
                    end: 2,
                }),
            },
        ],
    }]
}
