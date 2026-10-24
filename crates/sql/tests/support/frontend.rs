use xadb_foundation::runtime::{Cancellation, Clock};
use xadb_sql::frontend::{CallControl, Diagnostic, Keyword, Lexer, Token, TokenKind};
use xadbtest_reference::frontend::literals::{
    DiagnosticObservation, LexCase, LexObservation, TokenObservation,
};
use xadbtest_reference::frontend::{Verdict, check_lex};

pub struct FixedClock;
impl Clock for FixedClock {
    fn now_nanos(&self) -> u64 {
        0
    }
}

pub fn diagnostic_observation(value: Diagnostic) -> DiagnosticObservation {
    use xadb_sql::frontend::{Code, Module};
    DiagnosticObservation {
        code: match value.code {
            Code::Syntax => "Syntax",
            Code::Unsupported => "Unsupported",
            Code::Name => "Name",
            Code::Limit => "Limit",
            Code::Cancelled => "Cancelled",
            Code::Timeout => "Timeout",
            Code::Type => "Type",
            Code::Range => "Range",
            Code::NotNull => "NotNull",
        },
        module: match value.module {
            Module::Lex => "Lex",
            Module::Name => "Name",
            Module::Split => "Split",
            Module::Parse => "Parse",
            Module::Value => "Value",
        },
        start: value.span.start,
        end: value.span.end,
    }
}

pub fn token_observation(value: Token) -> TokenObservation {
    let kind = match value.kind {
        TokenKind::Keyword(keyword) => match keyword {
            Keyword::Create => "Keyword(Create)",
            Keyword::Table => "Keyword(Table)",
            Keyword::Primary => "Keyword(Primary)",
            Keyword::Key => "Keyword(Key)",
            Keyword::Unique => "Keyword(Unique)",
            Keyword::Storage => "Keyword(Storage)",
            Keyword::Disk => "Keyword(Disk)",
            Keyword::Bigint => "Keyword(Bigint)",
            Keyword::Boolean => "Keyword(Boolean)",
            Keyword::Varchar => "Keyword(Varchar)",
            Keyword::Varbinary => "Keyword(Varbinary)",
            Keyword::Index => "Keyword(Index)",
            Keyword::On => "Keyword(On)",
            Keyword::Drop => "Keyword(Drop)",
            Keyword::Insert => "Keyword(Insert)",
            Keyword::Into => "Keyword(Into)",
            Keyword::Values => "Keyword(Values)",
            Keyword::Update => "Keyword(Update)",
            Keyword::Set => "Keyword(Set)",
            Keyword::Delete => "Keyword(Delete)",
            Keyword::From => "Keyword(From)",
            Keyword::Select => "Keyword(Select)",
            Keyword::As => "Keyword(As)",
            Keyword::Where => "Keyword(Where)",
            Keyword::Order => "Keyword(Order)",
            Keyword::By => "Keyword(By)",
            Keyword::Asc => "Keyword(Asc)",
            Keyword::Desc => "Keyword(Desc)",
            Keyword::Nulls => "Keyword(Nulls)",
            Keyword::First => "Keyword(First)",
            Keyword::Last => "Keyword(Last)",
            Keyword::Limit => "Keyword(Limit)",
            Keyword::Not => "Keyword(Not)",
            Keyword::And => "Keyword(And)",
            Keyword::Or => "Keyword(Or)",
            Keyword::Is => "Keyword(Is)",
            Keyword::Null => "Keyword(Null)",
            Keyword::True => "Keyword(True)",
            Keyword::False => "Keyword(False)",
            Keyword::Begin => "Keyword(Begin)",
            Keyword::Commit => "Keyword(Commit)",
            Keyword::Rollback => "Keyword(Rollback)",
            Keyword::Autocommit => "Keyword(Autocommit)",
            Keyword::Off => "Keyword(Off)",
        },
        TokenKind::Identifier => "Identifier",
        TokenKind::QuotedIdentifier => "QuotedIdentifier",
        TokenKind::Digits => "Digits",
        TokenKind::Text => "Text",
        TokenKind::Binary => "Binary",
        TokenKind::Plus => "Plus",
        TokenKind::Minus => "Minus",
        TokenKind::Equal => "Equal",
        TokenKind::NotEqual => "NotEqual",
        TokenKind::Less => "Less",
        TokenKind::LessEqual => "LessEqual",
        TokenKind::Greater => "Greater",
        TokenKind::GreaterEqual => "GreaterEqual",
        TokenKind::LeftParen => "LeftParen",
        TokenKind::RightParen => "RightParen",
        TokenKind::Comma => "Comma",
        TokenKind::Dot => "Dot",
        TokenKind::Star => "Star",
        TokenKind::Semicolon => "Semicolon",
        TokenKind::Marker => "Marker",
        TokenKind::Eof => "Eof",
    };
    TokenObservation {
        kind,
        start: value.span.start,
        end: value.span.end,
    }
}

pub fn observe(source: &[u8]) -> LexObservation {
    let cancellation = Cancellation::default();
    let control = CallControl::new(&cancellation, &FixedClock, None);
    let mut observed = LexObservation {
        tokens: Vec::new(),
        diagnostic: None,
        complete: false,
    };
    let mut lexer = match Lexer::new(source, &control) {
        Ok(lexer) => lexer,
        Err(error) => {
            observed.diagnostic = Some(diagnostic_observation(error));
            observed.complete = true;
            return observed;
        }
    };
    // A legal input has at most 65,536 non-EOF tokens; one more iteration must
    // observe EOF or a limit diagnostic. The extra slot exposes overproduction.
    for _ in 0..65_538 {
        match lexer.next_token() {
            Ok(value) => {
                let eof = value.kind == TokenKind::Eof;
                observed.tokens.push(token_observation(value));
                if eof {
                    assert_eq!(
                        lexer.next_token(),
                        Ok(value),
                        "EOF changed on repeated read"
                    );
                    observed.complete = true;
                    break;
                }
            }
            Err(error) => {
                assert_eq!(
                    lexer.next_token(),
                    Err(error),
                    "terminal error changed on repeated read"
                );
                observed.diagnostic = Some(diagnostic_observation(error));
                observed.complete = true;
                break;
            }
        }
    }
    observed
}

pub fn assert_case(case: &LexCase) {
    let observed = observe(&case.source);
    assert_eq!(
        check_lex(case, &observed),
        Verdict::Accepted,
        "case={} observed={observed:?} expected={:?}",
        case.id,
        case.expected
    );
}
