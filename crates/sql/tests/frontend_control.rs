#![cfg(feature = "test-support")]

use foundation::runtime::{Cancellation, Clock};
use sql::frontend::{
    CallControl, Code, Diagnostic, Lexer, Module, Name, Span, Token, TokenKind, normalize_name,
};
use std::{cell::Cell, mem::size_of, sync::Barrier};

struct ManualClock(Cell<u64>);
impl Clock for ManualClock {
    fn now_nanos(&self) -> u64 {
        self.0.get()
    }
}

fn zero(position: usize) -> Span {
    Span {
        start: position,
        end: position,
    }
}

#[test]
fn entry_cancel_precedes_deadline_and_source_errors() {
    let cancel = Cancellation::default();
    cancel.cancel();
    let clock = ManualClock(Cell::new(7));
    let control = CallControl::new(&cancel, &clock, Some(7));
    let source = vec![0xff; 1_048_577];
    assert_eq!(
        Lexer::new(&source, &control).err(),
        Some(Diagnostic {
            code: Code::Cancelled,
            module: Module::Lex,
            span: zero(0),
        })
    );
}

#[test]
fn deadline_boundary_and_new_call_are_independent() {
    let cancel = Cancellation::default();
    let clock = ManualClock(Cell::new(6));
    let control = CallControl::new(&cancel, &clock, Some(7));
    assert!(Lexer::new(b"SELECT", &control).is_ok());
    for time in [7, 8] {
        clock.0.set(time);
        assert_eq!(
            Lexer::new(b"SELECT", &control).err(),
            Some(Diagnostic {
                code: Code::Timeout,
                module: Module::Lex,
                span: zero(0),
            })
        );
    }
    let fresh_cancel = Cancellation::default();
    let fresh_control = CallControl::new(&fresh_cancel, &clock, None);
    assert!(Lexer::new(b"SELECT", &fresh_control).is_ok());
}

#[test]
fn utf8_validation_checks_control_inside_long_input() {
    let cancel = Cancellation::default();
    let clock = ManualClock(Cell::new(0));
    let observer = |module: Module, span: Span| {
        assert_eq!(module, Module::Lex);
        if span.start >= 256 {
            cancel.cancel();
        }
    };
    let control = CallControl::new(&cancel, &clock, None).with_observer(&observer);
    let source = vec![b' '; 1_048_576];
    let error = Lexer::new(&source, &control).err().unwrap();
    assert_eq!(error.code, Code::Cancelled);
    assert_eq!(error.module, Module::Lex);
    assert_eq!(error.span.start, error.span.end);
    assert!((256..=259).contains(&error.span.start));
    let fresh_cancel = Cancellation::default();
    let fresh_control = CallControl::new(&fresh_cancel, &clock, None);
    assert!(Lexer::new(&source, &fresh_control).is_ok());
}

#[test]
fn long_tokens_and_comments_check_control_during_scan() {
    let long = "a".repeat(100_000);
    let sources = [
        "1".repeat(100_000),
        long.clone(),
        format!("'{long}'"),
        format!("\"{long}\""),
        format!("/*{long}*/"),
        format!("--{long}"),
    ];
    for (index, source) in sources.iter().enumerate() {
        let cancel = Cancellation::default();
        let clock = ManualClock(Cell::new(0));
        let armed = Cell::new(false);
        let observer = |module: Module, span: Span| {
            assert_eq!(module, Module::Lex);
            if armed.get() && span.start >= 256 {
                cancel.cancel();
            }
        };
        let control = CallControl::new(&cancel, &clock, None).with_observer(&observer);
        let mut lexer = Lexer::new(source.as_bytes(), &control).unwrap();
        armed.set(true);
        let error = lexer.next_token().unwrap_err();
        assert_eq!(error.code, Code::Cancelled, "source {index}");
        assert_eq!(error.module, Module::Lex);
        assert_eq!(error.span.start, error.span.end);
        assert!(
            (256..=259).contains(&error.span.start),
            "{index}: {error:?}"
        );
        assert!(error.span.start < source.len());
        clock.0.set(u64::MAX);
        assert_eq!(lexer.next_token().unwrap_err(), error);
    }
}

#[test]
fn deadline_inside_scan_and_eof_are_not_hidden() {
    let cancel = Cancellation::default();
    let clock = ManualClock(Cell::new(0));
    let armed = Cell::new(false);
    let observer = |_: Module, span: Span| {
        if armed.get() && span.start >= 256 {
            clock.0.set(5);
        }
    };
    let control = CallControl::new(&cancel, &clock, Some(5)).with_observer(&observer);
    let source = "1".repeat(100_000);
    let mut lexer = Lexer::new(source.as_bytes(), &control).unwrap();
    armed.set(true);
    let error = lexer.next_token().unwrap_err();
    assert_eq!(error.code, Code::Timeout);
    assert!((256..=259).contains(&error.span.start));
    assert_eq!(error.span.start, error.span.end);
    assert_eq!(lexer.next_token().unwrap_err(), error);
    clock.0.set(0);
    let eof_control = CallControl::new(&cancel, &clock, Some(5));
    let mut empty = Lexer::new(b"", &eof_control).unwrap();
    clock.0.set(5);
    assert_eq!(empty.next_token().unwrap_err().code, Code::Timeout);
}

#[test]
fn name_checks_control_and_rejects_forged_tokens() {
    let cancel = Cancellation::default();
    let clock = ManualClock(Cell::new(0));
    let control = CallControl::new(&cancel, &clock, Some(1));
    let token = Token {
        kind: TokenKind::Identifier,
        span: Span { start: 9, end: 3 },
    };
    cancel.cancel();
    assert_eq!(
        normalize_name(b"t", token, &control).unwrap_err(),
        Diagnostic {
            code: Code::Cancelled,
            module: Module::Name,
            span: zero(1),
        }
    );
    let fresh = Cancellation::default();
    let control = CallControl::new(&fresh, &clock, Some(1));
    clock.0.set(1);
    assert_eq!(
        normalize_name(b"t", token, &control).unwrap_err().code,
        Code::Timeout
    );
    clock.0.set(0);
    let cases: &[(&[u8], TokenKind, usize, usize)] = &[
        (b"t", TokenKind::Identifier, 1, 0),
        (b"t", TokenKind::Identifier, 0, 2),
        (b"t", TokenKind::Identifier, usize::MAX, usize::MAX),
        ("가".as_bytes(), TokenKind::Identifier, 1, 3),
        ("가".as_bytes(), TokenKind::Identifier, 0, 2),
        (b"t", TokenKind::Digits, 0, 1),
        (b"\"a\"b\"", TokenKind::QuotedIdentifier, 0, 5),
        (b"\"\xff\"", TokenKind::QuotedIdentifier, 0, 3),
    ];
    for &(source, kind, start, end) in cases {
        let token = Token {
            kind,
            span: Span { start, end },
        };
        let error = normalize_name(source, token, &control).unwrap_err();
        let start = start.min(source.len());
        assert_eq!(
            error,
            Diagnostic {
                code: Code::Name,
                module: Module::Name,
                span: Span {
                    start,
                    end: end.min(source.len()).max(start)
                },
            }
        );
    }
}

#[test]
fn frontend_values_have_bounded_stack_footprints() {
    assert!(size_of::<Lexer<'_, '_>>() <= 512);
    assert!(size_of::<Name>() <= 192);
    assert!(size_of::<Diagnostic>() <= 64);
}

#[test]
fn independent_calls_share_only_explicit_cancellation() {
    let cancel = Cancellation::default();
    let barrier = Barrier::new(3);
    std::thread::scope(|scope| {
        let mut workers = Vec::new();
        for source in [b"SELECT".as_slice(), b"COMMIT".as_slice()] {
            let token = cancel.clone();
            let barrier = &barrier;
            workers.push(scope.spawn(move || {
                let clock = ManualClock(Cell::new(0));
                let control = CallControl::new(&token, &clock, None);
                barrier.wait();
                Lexer::new(source, &control).err().unwrap().code
            }));
        }
        cancel.cancel();
        barrier.wait();
        for worker in workers {
            assert_eq!(worker.join().unwrap(), Code::Cancelled);
        }
    });
    let fresh = Cancellation::default();
    let clock = ManualClock(Cell::new(0));
    let control = CallControl::new(&fresh, &clock, None);
    assert!(Lexer::new(b"SELECT", &control).is_ok());
}

#[test]
fn name_decode_checks_control_after_validation() {
    let source = format!("\"{}\"", "\"\"".repeat(128));
    assert_eq!(source.len(), 258);
    let token = Token {
        kind: TokenKind::QuotedIdentifier,
        span: Span { start: 0, end: 258 },
    };
    let cancel = Cancellation::default();
    let clock = ManualClock(Cell::new(0));
    let validated = Cell::new(false);
    let observer = |module: Module, span: Span| {
        assert_eq!(module, Module::Name);
        if span.start == 258 {
            validated.set(true);
        } else if validated.get() && (256..258).contains(&span.start) {
            cancel.cancel();
        }
    };
    let control = CallControl::new(&cancel, &clock, None).with_observer(&observer);
    let error = normalize_name(source.as_bytes(), token, &control).unwrap_err();
    assert!(validated.get());
    assert_eq!(error.code, Code::Cancelled);
    assert!((256..258).contains(&error.span.start));
    assert_eq!(error.span.start, error.span.end);
    let fresh = Cancellation::default();
    let control = CallControl::new(&fresh, &clock, None);
    assert_eq!(
        normalize_name(source.as_bytes(), token, &control)
            .unwrap()
            .as_bytes(),
        &[b'"'; 128]
    );
}

#[test]
fn ascii_name_validation_checkpoints_do_not_use_unicode_slack() {
    let source = format!("\"{}\"", "\"\"".repeat(128));
    let token = Token {
        kind: TokenKind::QuotedIdentifier,
        span: Span { start: 0, end: 258 },
    };
    let cancel = Cancellation::default();
    let clock = ManualClock(Cell::new(0));
    let observer = |module: Module, span: Span| {
        assert_eq!(module, Module::Name);
        if span.start >= 256 {
            cancel.cancel();
        }
    };
    let control = CallControl::new(&cancel, &clock, None).with_observer(&observer);
    let error = normalize_name(source.as_bytes(), token, &control).unwrap_err();
    assert_eq!(error.code, Code::Cancelled);
    assert_eq!(error.span, zero(256));
}
