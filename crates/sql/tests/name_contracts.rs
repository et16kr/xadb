use xadb_foundation::runtime::{Cancellation, Clock};
use xadb_sql::frontend::{
    CallControl, Code, Keyword, Module, Span, Token, TokenKind, normalize_name,
};
use xadbtest_reference::frontend::literals::{
    DiagnosticObservation, NameObservation, name_literals,
};
use xadbtest_reference::frontend::{Verdict, check_name};

struct FixedClock;
impl Clock for FixedClock {
    fn now_nanos(&self) -> u64 {
        0
    }
}

#[test]
fn literal_name_vectors() {
    let cases = name_literals();
    assert_eq!(cases.len(), 110);
    let cancellation = Cancellation::default();
    let control = CallControl::new(&cancellation, &FixedClock, None);
    for case in &cases {
        let token = Token {
            kind: match case.token.kind {
                "Identifier" => TokenKind::Identifier,
                "QuotedIdentifier" => TokenKind::QuotedIdentifier,
                "Digits" => TokenKind::Digits,
                "Keyword(Select)" => TokenKind::Keyword(Keyword::Select),
                other => panic!("unexpected fixture input token {other}"),
            },
            span: Span {
                start: case.token.start,
                end: case.token.end,
            },
        };
        let observed = normalize_name(&case.source, token, &control)
            .map(|value| NameObservation {
                bytes: value.as_bytes().to_vec(),
                start: value.span().start,
                end: value.span().end,
                quoted: value.is_quoted(),
            })
            .map_err(|error| DiagnosticObservation {
                code: match error.code {
                    Code::Name => "Name",
                    Code::Syntax => "Syntax",
                    Code::Unsupported => "Unsupported",
                    Code::Limit => "Limit",
                    Code::Cancelled => "Cancelled",
                    Code::Timeout => "Timeout",
                    Code::Type => "Type",
                    Code::Range => "Range",
                    Code::NotNull => "NotNull",
                },
                module: match error.module {
                    Module::Lex => "Lex",
                    Module::Name => "Name",
                    Module::Split => "Split",
                    Module::Parse => "Parse",
                    Module::Value => "Value",
                },
                start: error.span.start,
                end: error.span.end,
            });
        assert_eq!(
            check_name(case, &observed),
            Verdict::Accepted,
            "case={} observed={observed:?} expected={:?}",
            case.id,
            case.expected
        );
    }
}
