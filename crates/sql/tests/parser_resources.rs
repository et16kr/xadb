//! Resource obligations frozen in tests/fixtures/dev4-parser/control.json.
#![cfg(feature = "test-support")]

use foundation::runtime::{Cancellation, Clock, Quota};
use sql::frontend::{
    AstNode, CallControl, Code, Diagnostic, Module, ParsedStatement, Parser, Span, SplitOutcome,
    Splitter,
};
use std::{cell::Cell, mem::size_of, sync::Barrier};

const SESSION: u64 = 8 << 20;
const DATABASE: u64 = 64 << 20;

#[derive(Default)]
struct TestClock(Cell<u64>);
impl Clock for TestClock {
    fn now_nanos(&self) -> u64 {
        self.0.get()
    }
}

fn parse(parser: &mut Parser, input: &[u8]) -> Result<ParsedStatement, Diagnostic> {
    parser.parse(
        input,
        &CallControl::new(&Cancellation::default(), &TestClock::default(), None),
    )
}

fn insert(rows: usize) -> Vec<u8> {
    let mut input = b"INSERT INTO t(a) VALUES".to_vec();
    for i in 0..rows {
        if i != 0 {
            input.push(b',');
        }
        input.extend_from_slice(b"(0)");
    }
    input
}

fn s() -> u64 {
    size_of::<AstNode>() as u64
}

fn charge(value: &ParsedStatement) -> u64 {
    (value.source_capacity_bytes() + value.arena_capacity_bytes()) as u64
}

fn error(code: Code, position: usize) -> Diagnostic {
    Diagnostic {
        code,
        module: Module::Parse,
        span: Span {
            start: position,
            end: position,
        },
    }
}

fn usage(session: &Quota, database: &Quota, expected: u64) {
    assert_eq!(session.used(), expected);
    assert_eq!(database.used(), expected);
}

fn constrained(local: bool, limit: u64) -> (Quota, Quota) {
    (
        Quota::new(if local { limit } else { SESSION }),
        Quota::new(if local { DATABASE } else { limit }),
    )
}

#[test]
fn source_and_ast_lifetimes() {
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut parser = Parser::new(&session, &database);
    usage(&session, &database, 0);
    let expected = b" \tBEGIN; \n";
    let mut input = expected.to_vec();
    let first = parse(&mut parser, &input).unwrap();
    input.fill(b'x');
    drop(input);
    assert_eq!(first.source(), expected);
    assert_eq!(first.nodes()[0].span(), Span { start: 2, end: 7 });
    let c1 = charge(&first);
    usage(&session, &database, c1);
    let second = parse(&mut parser, b"COMMIT").unwrap();
    let c2 = charge(&second);
    usage(&session, &database, c1 + c2);
    drop(parser);
    usage(&session, &database, c1 + c2);
    drop(first);
    assert_eq!(second.source(), b"COMMIT");
    usage(&session, &database, c2);
    drop(second);
    usage(&session, &database, 0);
    eprintln!("node size {}; retained charges {c1}, {c2}", s());
}

#[test]
fn source_reservation_boundaries() {
    for local in [true, false] {
        for limit in [4, 5, 6] {
            let (session, database) = constrained(local, limit);
            let mut parser = Parser::new(&session, &database);
            assert_eq!(
                parse(&mut parser, b"BEGIN").unwrap_err(),
                error(Code::Limit, 0)
            );
            assert_eq!(parser.allocation_count(), usize::from(limit >= 5));
            usage(&session, &database, 0);
            let expected_session_peak = if local {
                if limit < 5 { 0 } else { 5 }
            } else if limit < 5 {
                5
            } else {
                5 + 16 * s()
            };
            assert_eq!(session.peak(), expected_session_peak);
            assert_eq!(database.peak(), if limit < 5 { 0 } else { 5 });
        }
    }
}

#[test]
fn initial_arena_reservation_boundaries() {
    for local in [true, false] {
        for delta in [-1_i64, 0, 1] {
            let bound = 5 + 16 * s();
            let (session, database) = constrained(local, bound.checked_add_signed(delta).unwrap());
            let mut parser = Parser::new(&session, &database);
            let result = parse(&mut parser, b"BEGIN");
            if delta < 0 {
                assert_eq!(result.unwrap_err(), error(Code::Limit, 0));
                assert_eq!(parser.allocation_count(), 1);
            } else {
                let value = result.unwrap();
                assert_eq!(charge(&value), bound);
                assert_eq!(parser.allocation_count(), 2);
                usage(&session, &database, bound);
                drop(value);
            }
            usage(&session, &database, 0);
        }
    }
}

#[test]
fn growth_reservation_boundaries() {
    let input = insert(6);
    assert_eq!(input.len(), 46);
    for local in [true, false] {
        for delta in [-1_i64, 0, 1] {
            let bound = input.len() as u64 + 48 * s();
            let (session, database) = constrained(local, bound.checked_add_signed(delta).unwrap());
            let mut parser = Parser::new(&session, &database);
            let witnessed = Cell::new(false);
            let interior = Cell::new(false);
            let observer = |module, span: Span| {
                if module == Module::Parse && database.used() == bound {
                    assert_eq!(session.used(), bound);
                    witnessed.set(true);
                }
                if module == Module::Parse && witnessed.get() && span.start == 25 {
                    assert_eq!(
                        session.used(),
                        bound,
                        "old backing still exists during move"
                    );
                    assert_eq!(
                        database.used(),
                        bound,
                        "old backing still exists during move"
                    );
                    interior.set(true);
                }
            };
            let token = Cancellation::default();
            let clock = TestClock::default();
            let control = CallControl::new(&token, &clock, None).with_observer(&observer);
            let result = parser.parse(&input, &control);
            if delta < 0 {
                assert_eq!(result.unwrap_err(), error(Code::Limit, 43));
                assert_eq!(parser.allocation_count(), 2);
            } else {
                let value = result.unwrap();
                assert!(witnessed.get(), "old and new arenas must coexist");
                assert!(
                    interior.get(),
                    "old and new must remain charged during move"
                );
                assert_eq!(value.nodes().len(), 18);
                assert_eq!(charge(&value), input.len() as u64 + 32 * s());
                assert_eq!(parser.allocation_count(), 3);
                usage(&session, &database, charge(&value));
                assert_eq!(database.peak(), bound);
                drop(value);
            }
            usage(&session, &database, 0);
        }
    }
}

#[test]
fn retained_ast_survives_next_failure() {
    let first_charge = 5 + 16 * s();
    let second_charge = 6 + 16 * s();
    for local in [true, false] {
        for delta in [-1_i64, 0, 1] {
            let (session, database) = constrained(
                local,
                (first_charge + second_charge)
                    .checked_add_signed(delta)
                    .unwrap(),
            );
            let mut parser = Parser::new(&session, &database);
            let first = parse(&mut parser, b"BEGIN").unwrap();
            let second = parse(&mut parser, b"COMMIT");
            if delta < 0 {
                assert_eq!(second.unwrap_err(), error(Code::Limit, 0));
                usage(&session, &database, first_charge);
            } else {
                let second = second.unwrap();
                usage(&session, &database, first_charge + second_charge);
                drop(second);
            }
            assert_eq!(first.source(), b"BEGIN");
            assert_eq!(first.nodes().len(), 1);
            usage(&session, &database, first_charge);
            drop(first);
            usage(&session, &database, 0);
        }
    }
}

#[test]
fn source_extra_capacity_failures() {
    for local in [true, false] {
        let (session, database) = constrained(local, 5);
        let mut parser = Parser::new(&session, &database);
        parser.set_allocation_padding(1);
        assert_eq!(
            parse(&mut parser, b"BEGIN").unwrap_err(),
            error(Code::Limit, 0)
        );
        assert_eq!(parser.allocation_count(), 1);
        assert_eq!(session.peak(), if local { 5 } else { 6 });
        assert_eq!(database.peak(), 5);
        usage(&session, &database, 0);
    }
}

#[test]
fn arena_extra_capacity_failures() {
    for local in [true, false] {
        let base = 6 + 16 * s();
        let (session, database) = constrained(local, base);
        let mut parser = Parser::new(&session, &database);
        parser.set_allocation_padding(1);
        assert_eq!(
            parse(&mut parser, b"BEGIN").unwrap_err(),
            error(Code::Limit, 0)
        );
        assert_eq!(parser.allocation_count(), 2);
        assert_eq!(session.peak(), if local { base } else { base + s() });
        assert_eq!(database.peak(), base);
        usage(&session, &database, 0);
    }
}

#[test]
fn actual_capacity_and_growth_padding() {
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut parser = Parser::new(&session, &database);
    parser.set_allocation_padding(1);
    let input = insert(10);
    let bound = input.len() as u64 + 1 + (17 + 35) * s();
    let witnessed = Cell::new(false);
    let observer = |module, _| {
        if module == Module::Parse && database.used() == bound {
            assert_eq!(session.used(), bound);
            witnessed.set(true);
        }
    };
    let token = Cancellation::default();
    let clock = TestClock::default();
    let control = CallControl::new(&token, &clock, None).with_observer(&observer);
    let value = parser.parse(&input, &control).unwrap();
    assert!(witnessed.get());
    assert_eq!(value.source_capacity_bytes(), input.len() + 1);
    assert_eq!(value.arena_capacity_bytes() as u64, 35 * s());
    usage(&session, &database, charge(&value));
    assert_eq!(database.peak(), bound);
    drop(value);
    let second = parse(&mut parser, b"BEGIN").unwrap();
    assert_eq!(second.source_capacity_bytes(), 6);
    assert_eq!(second.arena_capacity_bytes() as u64, 17 * s());
    usage(&session, &database, charge(&second));
    drop(second);
    parser.set_allocation_padding(0);
    let third = parse(&mut parser, b"BEGIN").unwrap();
    assert_eq!(charge(&third), 5 + 16 * s());
    drop(third);
    usage(&session, &database, 0);
    eprintln!(
        "padded source {}; arena17->35 nodes; peak{bound}",
        input.len() + 1
    );
}

#[test]
fn allocation_failure_at_every_backing() {
    let input = insert(4093);
    assert_eq!(input.len(), 16394);
    for ordinal in 1..=11 {
        let session = Quota::new(SESSION);
        let database = Quota::new(DATABASE);
        let mut parser = Parser::new(&session, &database);
        parser.fail_allocation_at(ordinal);
        let position = if ordinal < 3 {
            0
        } else {
            2 * (16 << (ordinal - 3)) + 11
        };
        assert_eq!(
            parse(&mut parser, &input).unwrap_err(),
            error(Code::Limit, position)
        );
        assert_eq!(parser.allocation_count(), ordinal);
        usage(&session, &database, 0);
        let next = parse(&mut parser, b"BEGIN").unwrap();
        assert_eq!(parser.allocation_count(), 2);
        drop(next);
        usage(&session, &database, 0);
    }
}

#[test]
fn unused_injection_does_not_escape_call() {
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut parser = Parser::new(&session, &database);
    for ordinal in [0, 99] {
        parser.fail_allocation_at(ordinal);
        drop(parse(&mut parser, b"BEGIN").unwrap());
        drop(parse(&mut parser, b"BEGIN").unwrap());
        usage(&session, &database, 0);
    }
    parser.fail_allocation_at(1);
    let token = Cancellation::default();
    token.cancel();
    assert_eq!(
        parser
            .parse(
                b"BEGIN",
                &CallControl::new(&token, &TestClock::default(), None)
            )
            .unwrap_err(),
        error(Code::Cancelled, 0)
    );
    assert_eq!(parser.allocation_count(), 0);
    drop(parse(&mut parser, b"BEGIN").unwrap());
    parser.fail_allocation_at(1);
    assert_eq!(parse(&mut parser, &[0xff]).unwrap_err().module, Module::Lex);
    assert_eq!(parser.allocation_count(), 0);
    drop(parse(&mut parser, b"BEGIN").unwrap());
    usage(&session, &database, 0);
}

#[test]
fn empty_input_has_no_backing() {
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut parser = Parser::new(&session, &database);
    parser.set_allocation_padding(99);
    parser.fail_allocation_at(1);
    assert_eq!(parse(&mut parser, b"").unwrap_err(), error(Code::Syntax, 0));
    assert_eq!(parser.allocation_count(), 0);
    assert_eq!(database.peak(), 0);
    parser.set_allocation_padding(0);
    assert_eq!(
        parse(&mut parser, b" ").unwrap_err(),
        error(Code::Syntax, 1)
    );
    assert_eq!(parser.allocation_count(), 1);
    usage(&session, &database, 0);
    drop(parse(&mut parser, b"BEGIN").unwrap());
    usage(&session, &database, 0);
}

fn copy_control(arena: bool) {
    for timeout in [false, true] {
        let session = Quota::new(SESSION);
        let database = Quota::new(DATABASE);
        let mut parser = Parser::new(&session, &database);
        let mut input = if arena { insert(6) } else { b"BEGIN".to_vec() };
        if !arena {
            input.resize(2048, b' ');
        }
        let target = if arena { 25 } else { 256 };
        let expected_charge = if arena {
            input.len() as u64 + 48 * s()
        } else {
            2048
        };
        let token = Cancellation::default();
        let clock = TestClock::default();
        let witnessed = Cell::new(false);
        let observer = |module, span: Span| {
            if module == Module::Parse && span.start == target && database.used() == expected_charge
            {
                assert_eq!(session.used(), expected_charge);
                witnessed.set(true);
                if timeout {
                    clock.0.set(9);
                } else {
                    token.cancel();
                }
            }
        };
        let control = CallControl::new(&token, &clock, Some(9)).with_observer(&observer);
        let result = parser.parse(&input, &control);
        assert!(witnessed.get(), "interior copy checkpoint absent");
        assert_eq!(
            result.unwrap_err(),
            error(
                if timeout {
                    Code::Timeout
                } else {
                    Code::Cancelled
                },
                target
            )
        );
        assert_eq!(parser.allocation_count(), if arena { 3 } else { 1 });
        usage(&session, &database, 0);
        drop(parse(&mut parser, b"BEGIN").unwrap());
        usage(&session, &database, 0);
    }
}

#[test]
fn source_copy_checkpoint_cleans_all() {
    copy_control(false);
}

#[test]
fn arena_move_checkpoint_cleans_old_new() {
    copy_control(true);
}

#[test]
fn entry_and_return_control() {
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut parser = Parser::new(&session, &database);
    let token = Cancellation::default();
    token.cancel();
    assert_eq!(
        parser
            .parse(
                &[0xff],
                &CallControl::new(&token, &TestClock::default(), Some(0))
            )
            .unwrap_err(),
        error(Code::Cancelled, 0)
    );
    assert_eq!(parser.allocation_count(), 0);
    for timeout in [false, true] {
        let token = Cancellation::default();
        let clock = TestClock::default();
        let observed = Cell::new(false);
        let observer = |module, span: Span| {
            if module == Module::Parse && span.start == 5 && database.used() == 5 + 16 * s() {
                observed.set(true);
                if timeout {
                    clock.0.set(9);
                } else {
                    token.cancel();
                    clock.0.set(9);
                }
            }
        };
        let control = CallControl::new(&token, &clock, Some(9)).with_observer(&observer);
        assert_eq!(
            parser.parse(b"BEGIN", &control).unwrap_err(),
            error(
                if timeout {
                    Code::Timeout
                } else {
                    Code::Cancelled
                },
                5
            )
        );
        assert!(observed.get());
        assert_eq!(parser.allocation_count(), 2);
        usage(&session, &database, 0);
    }
}

#[test]
fn nested_lex_and_name_failures_release_ast() {
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut parser = Parser::new(&session, &database);
    for (input, expected) in [
        (
            &b"INSERT INTO t(a) VALUES(1),(!=)"[..],
            Diagnostic {
                code: Code::Unsupported,
                module: Module::Lex,
                span: Span { start: 28, end: 30 },
            },
        ),
        (
            &b"INSERT INTO t(a,\"bad\0\") VALUES(1)"[..],
            Diagnostic {
                code: Code::Name,
                module: Module::Name,
                span: Span { start: 16, end: 22 },
            },
        ),
    ] {
        assert_eq!(parse(&mut parser, input).unwrap_err(), expected);
        assert!(parser.allocation_count() >= 2);
        usage(&session, &database, 0);
    }
    for module in [Module::Lex, Module::Name] {
        let token = Cancellation::default();
        let observed = Cell::new(None);
        let observer = |found, span: Span| {
            if found == module && database.used() > 26 {
                observed.set(Some(span));
                token.cancel();
            }
        };
        let clock = TestClock::default();
        let control = CallControl::new(&token, &clock, None).with_observer(&observer);
        let got = parser
            .parse(b"INSERT INTO t(a) VALUES(0)", &control)
            .unwrap_err();
        assert_eq!(
            got,
            Diagnostic {
                code: Code::Cancelled,
                module,
                span: observed.get().unwrap()
            }
        );
        usage(&session, &database, 0);
    }
    drop(parse(&mut parser, b"BEGIN").unwrap());
    usage(&session, &database, 0);
}

#[test]
fn calls_borrow_only_current_control() {
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut parser = Parser::new(&session, &database);
    let token1 = Cancellation::default();
    let token2 = Cancellation::default();
    let clock = TestClock::default();
    let first = parser
        .parse(b"BEGIN", &CallControl::new(&token1, &clock, None))
        .unwrap();
    token1.cancel();
    let second = parser
        .parse(b"COMMIT", &CallControl::new(&token2, &clock, None))
        .unwrap();
    token2.cancel();
    let third = parse(&mut parser, b"ROLLBACK").unwrap();
    assert_eq!(first.source(), b"BEGIN");
    assert_eq!(second.source(), b"COMMIT");
    assert_eq!(third.source(), b"ROLLBACK");
    usage(
        &session,
        &database,
        charge(&first) + charge(&second) + charge(&third),
    );
    drop((first, second, third));
    usage(&session, &database, 0);
}

#[test]
fn splitter_and_parser_charge_same_ledgers() {
    for source_first in [true, false] {
        let session = Quota::new(SESSION);
        let database = Quota::new(DATABASE);
        let mut splitter = Splitter::new(&session, &database);
        let token = Cancellation::default();
        let clock = TestClock::default();
        let control = CallControl::new(&token, &clock, None);
        let SplitOutcome::Statement(source) = splitter.push(b"BEGIN;", &control).unwrap().outcome
        else {
            panic!("expected complete source");
        };
        let mut parser = Parser::new(&session, &database);
        let value = parser.parse(source.as_bytes(), &control).unwrap();
        let a = source.capacity_bytes() as u64;
        let b = charge(&value);
        usage(&session, &database, a + b);
        if source_first {
            drop(source);
            usage(&session, &database, b);
            assert_eq!(value.source(), b"BEGIN;");
            drop(value);
        } else {
            drop(value);
            usage(&session, &database, a);
            assert_eq!(source.as_bytes(), b"BEGIN;");
            drop(source);
        }
        usage(&session, &database, 0);
    }
}

#[test]
fn shared_database_callers() {
    let one = 5 + 16 * s();
    for delta in [-1_i64, 0, 1] {
        let database = Quota::new((2 * one).checked_add_signed(delta).unwrap());
        let a = Quota::new(SESSION);
        let b = Quota::new(SESSION);
        let mut pa = Parser::new(&a, &database);
        let mut pb = Parser::new(&b, &database);
        let first = parse(&mut pa, b"BEGIN").unwrap();
        let second = parse(&mut pb, b"BEGIN");
        if delta < 0 {
            assert_eq!(second.unwrap_err(), error(Code::Limit, 0));
            assert_eq!(b.used(), 0);
        } else {
            let second = second.unwrap();
            assert_eq!(a.used() + b.used(), database.used());
            drop(second);
        }
        assert_eq!(first.source(), b"BEGIN");
        assert_eq!(a.used(), one);
        assert_eq!(database.used(), one);
        drop(first);
        assert_eq!(database.used(), 0);
    }
}

#[test]
fn native_threads_share_database() {
    let bound = 10 + 16 * s();
    let database = Quota::new(bound);
    let barrier = Barrier::new(2);
    let outcomes = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let database = &database;
                let barrier = &barrier;
                scope.spawn(move || {
                    let session = Quota::new(SESSION);
                    let mut parser = Parser::new(&session, database);
                    barrier.wait();
                    let result = parse(&mut parser, b"BEGIN");
                    // Returning keeps successful allocations alive in the join
                    // result. No post-parse barrier can strand a peer on panic.
                    (session, result)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join())
            .collect::<Vec<_>>()
    });
    let outcomes: Vec<_> = outcomes.into_iter().map(Result::unwrap).collect();
    assert_eq!(
        outcomes.iter().filter(|(_, value)| value.is_ok()).count(),
        1
    );
    assert!(database.used() <= bound);
    for (session, result) in outcomes {
        if let Ok(value) = &result {
            assert_eq!(value.source(), b"BEGIN");
            assert_eq!(session.used(), charge(value));
        } else {
            assert_eq!(result.as_ref().unwrap_err().code, Code::Limit);
            assert_eq!(session.used(), 0);
        }
        drop(result);
        assert_eq!(session.used(), 0);
    }
    assert_eq!(database.used(), 0);
}

#[test]
fn maximum_flat_ast_drop() {
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let a = session.clone();
    let b = database.clone();
    std::thread::Builder::new()
        .stack_size(256 << 10)
        .spawn(move || {
            let mut parser = Parser::new(&a, &b);
            let value = parse(&mut parser, &insert(4093)).unwrap();
            assert_eq!(value.nodes().len(), 8192);
            assert_eq!(parser.allocation_count(), 11);
            usage(&a, &b, charge(&value));
            drop(value);
            usage(&a, &b, 0);
        })
        .unwrap()
        .join()
        .unwrap();
    usage(&session, &database, 0);
}

#[test]
fn parser_stack_footprints() {
    assert!(size_of::<Parser>() <= 128);
    assert!(size_of::<ParsedStatement>() <= 384);
    assert!(size_of::<AstNode>() <= 256);
    eprintln!(
        "Parser {}, ParsedStatement {}, AstNode {}",
        size_of::<Parser>(),
        size_of::<ParsedStatement>(),
        size_of::<AstNode>()
    );
}

#[test]
fn padding_overflow_cleans_reservations() {
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut parser = Parser::new(&session, &database);
    parser.set_allocation_padding(usize::MAX);
    assert_eq!(
        parse(&mut parser, b"BEGIN").unwrap_err(),
        error(Code::Limit, 0)
    );
    assert_eq!(parser.allocation_count(), 1);
    usage(&session, &database, 0);
    parser.set_allocation_padding(0);
    drop(parse(&mut parser, b"BEGIN").unwrap());
    usage(&session, &database, 0);
}

#[test]
fn exponent_classification_control_is_bounded() {
    for timeout in [false, true] {
        let session = Quota::new(SESSION);
        let database = Quota::new(DATABASE);
        let mut parser = Parser::new(&session, &database);
        let mut input = b"INSERT INTO t(a) VALUES(1e".to_vec();
        input.extend(std::iter::repeat_n(b'7', 4096));
        input.push(b')');
        let expected_charge = input.len() as u64 + 16 * s();
        let token = Cancellation::default();
        let clock = TestClock::default();
        let observed = Cell::new(None);
        let observer = |module, span: Span| {
            if module == Module::Parse && database.used() == expected_charge && span.start >= 700 {
                assert_eq!(session.used(), expected_charge);
                observed.set(Some(span.start));
                if timeout {
                    clock.0.set(9);
                } else {
                    token.cancel();
                }
            }
        };
        let control = CallControl::new(&token, &clock, Some(9)).with_observer(&observer);
        let result = parser.parse(&input, &control);
        let p = observed
            .get()
            .expect("interior exponent classification checkpoint");
        assert!((700..=956).contains(&p), "unbounded classification: {p}");
        assert_eq!(
            result.unwrap_err(),
            error(
                if timeout {
                    Code::Timeout
                } else {
                    Code::Cancelled
                },
                p
            )
        );
        usage(&session, &database, 0);
        drop(parse(&mut parser, b"BEGIN").unwrap());
        usage(&session, &database, 0);
    }
}
