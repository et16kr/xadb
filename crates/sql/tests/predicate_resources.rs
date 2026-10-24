//! New predicate entry/resource obligations; prior parser suites are not rerun.
#![cfg(feature = "test-support")]

use std::{cell::Cell, mem::size_of, sync::Barrier};
use xadb_foundation::runtime::{Cancellation, Clock, Quota};
use xadb_sql::frontend::{
    AstNode, CallControl, Code, DecodedScalar, Diagnostic, Module, ParsedStatement, Parser,
    ScalarReader, ScalarRef, Span,
};

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
    parser.parse_predicate(
        input,
        &CallControl::new(&Cancellation::default(), &TestClock::default(), None),
    )
}

fn chain(count: usize, operator: &str) -> Vec<u8> {
    let mut source = String::from("0=0");
    for _ in 1..count {
        source.push(' ');
        source.push_str(operator);
        source.push_str(" 0=0");
    }
    source.into_bytes()
}

fn node_bytes() -> u64 {
    size_of::<AstNode>() as u64
}

fn charge(tree: &ParsedStatement) -> u64 {
    (tree.source_capacity_bytes() + tree.arena_capacity_bytes()) as u64
}

fn usage(session: &Quota, database: &Quota, bytes: u64) {
    assert_eq!(session.used(), bytes);
    assert_eq!(database.used(), bytes);
}

fn limit(at: usize) -> Diagnostic {
    Diagnostic {
        code: Code::Limit,
        module: Module::Parse,
        span: Span { start: at, end: at },
    }
}

fn quotas(local: bool, bound: u64) -> (Quota, Quota) {
    (
        Quota::new(if local { bound } else { SESSION }),
        Quota::new(if local { DATABASE } else { bound }),
    )
}

#[test]
fn predicate_owned_lifetimes() {
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut parser = Parser::new(&session, &database);
    let expected = b"a = 'x' AND ? IS NULL";
    let mut source = expected.to_vec();
    let first = parse(&mut parser, &source).unwrap();
    source.fill(b'!');
    drop(source);
    assert_eq!(first.source(), expected);
    assert_eq!(first.nodes().len(), 8);
    assert_eq!(first.marker_count(), 1);
    let a = charge(&first);
    let second = parse(&mut parser, b"NOT (NULL IS NULL)").unwrap();
    let b = charge(&second);
    let mut reader = ScalarReader::new(&session, &database);
    let scalar = reader
        .decode(
            &first,
            5,
            &CallControl::new(&Cancellation::default(), &TestClock::default(), None),
        )
        .unwrap();
    let DecodedScalar::Value(scalar) = scalar else {
        panic!("raw predicate text node must decode as a value");
    };
    assert_eq!(scalar.as_ref(), ScalarRef::Utf8("x"));
    let c = scalar.capacity_bytes() as u64;
    usage(&session, &database, a + b + c);
    drop(parser);
    drop(first);
    assert_eq!(scalar.as_ref(), ScalarRef::Utf8("x"));
    assert_eq!(second.source(), b"NOT (NULL IS NULL)");
    usage(&session, &database, b + c);
    drop(second);
    drop(reader);
    usage(&session, &database, c);
    drop(scalar);
    usage(&session, &database, 0);
}

#[test]
fn predicate_quota_boundaries() {
    for local in [true, false] {
        for delta in [-1_i64, 0, 1] {
            let bound = 3 + 16 * node_bytes();
            let (session, database) = quotas(local, bound.checked_add_signed(delta).unwrap());
            let mut parser = Parser::new(&session, &database);
            let result = parse(&mut parser, b"0=0");
            if delta < 0 {
                assert_eq!(result.unwrap_err(), limit(0));
                assert_eq!(session.peak(), if local { 3 } else { bound });
                assert_eq!(database.peak(), 3);
            } else {
                let tree = result.unwrap();
                assert_eq!(charge(&tree), bound);
                usage(&session, &database, bound);
                drop(tree);
                assert_eq!(session.peak(), bound);
                assert_eq!(database.peak(), bound);
            }
            usage(&session, &database, 0);
        }
        for delta in [-1_i64, 0, 1] {
            let retained = 3 + 16 * node_bytes();
            let peak = retained + 35 + 48 * node_bytes();
            let (session, database) = quotas(local, peak.checked_add_signed(delta).unwrap());
            let mut parser = Parser::new(&session, &database);
            let old = parse(&mut parser, b"0=0").unwrap();
            let result = parse(&mut parser, &chain(5, "AND"));
            if delta < 0 {
                assert_eq!(result.unwrap_err(), limit(32));
                let before_growth = retained + 35 + 16 * node_bytes();
                assert_eq!(session.peak(), if local { before_growth } else { peak });
                assert_eq!(database.peak(), before_growth);
            } else {
                let tree = result.unwrap();
                assert_eq!(tree.nodes().len(), 18);
                assert_eq!(charge(&tree), 35 + 32 * node_bytes());
                usage(&session, &database, retained + charge(&tree));
                drop(tree);
                assert_eq!(session.peak(), peak);
                assert_eq!(database.peak(), peak);
            }
            usage(&session, &database, retained);
            assert_eq!(old.source(), b"0=0");
            drop(old);
            usage(&session, &database, 0);
        }
    }
}

#[test]
fn predicate_actual_capacity() {
    for local in [true, false] {
        for delta in [-1_i64, 0, 1] {
            let bound = 4 + 17 * node_bytes();
            let (session, database) = quotas(local, bound.checked_add_signed(delta).unwrap());
            let mut parser = Parser::new(&session, &database);
            parser.set_allocation_padding(1);
            let result = parse(&mut parser, b"0=0");
            if delta < 0 {
                assert_eq!(result.unwrap_err(), limit(0));
                let base = 4 + 16 * node_bytes();
                assert_eq!(session.peak(), if local { base } else { bound });
                assert_eq!(database.peak(), base);
            } else {
                let tree = result.unwrap();
                assert_eq!(tree.source_capacity_bytes(), 4);
                assert_eq!(tree.arena_capacity_bytes() as u64, 17 * node_bytes());
                usage(&session, &database, bound);
                drop(tree);
                assert_eq!(session.peak(), bound);
                assert_eq!(database.peak(), bound);
            }
            assert_eq!(parser.allocation_count(), 2);
            usage(&session, &database, 0);
        }
    }
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut parser = Parser::new(&session, &database);
    parser.set_allocation_padding(1);
    let a = parse(&mut parser, b"0=0").unwrap();
    let b = parse(&mut parser, b"0=0").unwrap();
    assert_eq!(charge(&a), 4 + 17 * node_bytes());
    assert_eq!(charge(&a), charge(&b));
    parser.set_allocation_padding(0);
    let c = parse(&mut parser, b"0=0").unwrap();
    assert_eq!(charge(&c), 3 + 16 * node_bytes());
    let held = charge(&a) + charge(&b) + charge(&c);
    parser.set_allocation_padding(usize::MAX);
    assert_eq!(parse(&mut parser, b"0=0").unwrap_err(), limit(0));
    assert_eq!(parser.allocation_count(), 1);
    usage(&session, &database, held);
    drop((a, b, c));
    usage(&session, &database, 0);
}

#[test]
fn predicate_allocation_failures() {
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut parser = Parser::new(&session, &database);
    let old = parse(&mut parser, b"0=0").unwrap();
    let held = charge(&old);
    let input = chain(300, "AND");
    for (site, position) in [0, 0, 32, 74, 160, 330, 672, 1354].into_iter().enumerate() {
        let ordinal = site + 1;
        parser.fail_allocation_at(ordinal);
        assert_eq!(parse(&mut parser, &input).unwrap_err(), limit(position));
        assert_eq!(parser.allocation_count(), ordinal);
        usage(&session, &database, held);
        assert_eq!(old.source(), b"0=0");
        drop(parse(&mut parser, b"1=1").unwrap());
        usage(&session, &database, held);
    }
    parser.fail_allocation_at(9);
    let complete = parse(&mut parser, &input).unwrap();
    assert_eq!(complete.nodes().len(), 903);
    assert_eq!(parser.allocation_count(), 8);
    drop(complete);
    // This next call reaches site nine: a wrongly retained injection would fail.
    let later = parse(&mut parser, &chain(400, "AND")).unwrap();
    assert_eq!(parser.allocation_count(), 9);
    drop(later);
    usage(&session, &database, held);
    drop(old);
    usage(&session, &database, 0);
}

#[test]
fn predicate_entry_and_final_controls() {
    let empty_session = Quota::new(0);
    let empty_database = Quota::new(0);
    let mut empty_parser = Parser::new(&empty_session, &empty_database);
    assert_eq!(
        parse(&mut empty_parser, b"").unwrap_err(),
        Diagnostic {
            code: Code::Syntax,
            module: Module::Parse,
            span: Span { start: 0, end: 0 }
        }
    );
    assert_eq!(empty_parser.allocation_count(), 0);
    assert_eq!(parse(&mut empty_parser, b" ").unwrap_err(), limit(0));
    usage(&empty_session, &empty_database, 0);
    for local in [true, false] {
        let (session, database) = quotas(local, 1 + 16 * node_bytes() - 1);
        let mut parser = Parser::new(&session, &database);
        assert_eq!(parse(&mut parser, b";").unwrap_err(), limit(0));
        assert_eq!(parser.allocation_count(), 1);
        usage(&session, &database, 0);
    }
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut parser = Parser::new(&session, &database);
    parser.fail_allocation_at(2);
    assert_eq!(parse(&mut parser, b";").unwrap_err(), limit(0));
    assert_eq!(parser.allocation_count(), 2);
    assert_eq!(
        parse(&mut parser, b";").unwrap_err(),
        Diagnostic {
            code: Code::Syntax,
            module: Module::Parse,
            span: Span { start: 0, end: 1 }
        }
    );
    usage(&session, &database, 0);
    let clock = TestClock(Cell::new(10));
    let cancelled = Cancellation::default();
    cancelled.cancel();
    parser.fail_allocation_at(1);
    let error = parser
        .parse_predicate(b"???", &CallControl::new(&cancelled, &clock, Some(10)))
        .unwrap_err();
    assert_eq!(
        error,
        Diagnostic {
            code: Code::Cancelled,
            module: Module::Parse,
            span: Span { start: 0, end: 0 }
        }
    );
    assert_eq!(parser.allocation_count(), 0);
    drop(parse(&mut parser, b"0=0").unwrap());
    let timeout = parser
        .parse_predicate(
            b"0=0",
            &CallControl::new(&Cancellation::default(), &clock, Some(10)),
        )
        .unwrap_err();
    assert_eq!(
        timeout,
        Diagnostic {
            code: Code::Timeout,
            module: Module::Parse,
            span: Span { start: 0, end: 0 }
        }
    );
    for use_timeout in [false, true] {
        let cancellation = Cancellation::default();
        let clock = TestClock::default();
        let input = b"0=0; /* trailing */  ";
        let publication_checks = Cell::new(0);
        let observer = |module, span: Span| {
            // Source-copy completion has no arena. This phase is selected from
            // held allocations, not an incidental count of helper callbacks.
            if module == Module::Parse
                && span.start == input.len()
                && database.used() == input.len() as u64 + 16 * node_bytes()
            {
                publication_checks.set(publication_checks.get() + 1);
                if use_timeout {
                    clock.0.set(10);
                } else {
                    cancellation.cancel();
                }
            }
        };
        let result = parser.parse_predicate(
            input,
            &CallControl::new(&cancellation, &clock, Some(10)).with_observer(&observer),
        );
        assert_eq!(
            publication_checks.get(),
            1,
            "final predicate publication checkpoint"
        );
        let error = result.unwrap_err();
        assert_eq!(
            error.code,
            if use_timeout {
                Code::Timeout
            } else {
                Code::Cancelled
            }
        );
        assert_eq!(error.module, Module::Parse);
        assert_eq!(
            error.span,
            Span {
                start: input.len(),
                end: input.len()
            }
        );
        usage(&session, &database, 0);
        drop(parse(&mut parser, b"0=0").unwrap());
    }
}

#[test]
fn predicate_interior_controls() {
    for operator in ["AND", "OR"] {
        for use_timeout in [false, true] {
            let session = Quota::new(SESSION);
            let database = Quota::new(DATABASE);
            let mut parser = Parser::new(&session, &database);
            let input = chain(300, operator);
            let copy_done = Cell::new(false);
            let witness = Cell::new(None);
            let cancellation = Cancellation::default();
            let clock = TestClock::default();
            let observer = |module, span: Span| {
                if module != Module::Parse {
                    return;
                }
                if span.start == input.len() {
                    copy_done.set(true);
                    return;
                }
                if copy_done.get() && span.start >= 1024 && span.start < input.len() {
                    witness.set(Some(span.start));
                    if use_timeout {
                        clock.0.set(10);
                    } else {
                        cancellation.cancel();
                    }
                }
            };
            let error = parser
                .parse_predicate(
                    &input,
                    &CallControl::new(&cancellation, &clock, Some(10)).with_observer(&observer),
                )
                .unwrap_err();
            assert_eq!(
                error.code,
                if use_timeout {
                    Code::Timeout
                } else {
                    Code::Cancelled
                }
            );
            assert_eq!(error.module, Module::Parse);
            assert!(
                matches!(witness.get(), Some(1024..=1280)),
                "new predicate loop not reached"
            );
            let position = witness.get().unwrap();
            assert_eq!(
                error.span,
                Span {
                    start: position,
                    end: position
                }
            );
            usage(&session, &database, 0);
            drop(parse(&mut parser, b"0=0").unwrap());
        }
    }
    for (groups, threshold) in [(false, 128), (true, 32), (true, 100)] {
        for use_timeout in [false, true] {
            let session = Quota::new(SESSION);
            let database = Quota::new(DATABASE);
            let mut parser = Parser::new(&session, &database);
            let input = if groups {
                format!("{}0=0{}", "(".repeat(64), ")".repeat(64))
            } else {
                format!("{}0=0", "NOT ".repeat(64))
            };
            let copy_done = Cell::new(false);
            let witness = Cell::new(None);
            let cancellation = Cancellation::default();
            let clock = TestClock::default();
            let observer = |module, span: Span| {
                if module != Module::Parse {
                    return;
                }
                if span.start == input.len() {
                    copy_done.set(true);
                    return;
                }
                if copy_done.get() && span.start >= threshold && span.start < input.len() {
                    witness.set(Some(span.start));
                    if use_timeout {
                        clock.0.set(10);
                    } else {
                        cancellation.cancel();
                    }
                }
            };
            let error = parser
                .parse_predicate(
                    input.as_bytes(),
                    &CallControl::new(&cancellation, &clock, Some(10)).with_observer(&observer),
                )
                .unwrap_err();
            assert_eq!(
                error.code,
                if use_timeout {
                    Code::Timeout
                } else {
                    Code::Cancelled
                }
            );
            assert_eq!(error.module, Module::Parse);
            assert_eq!(
                witness.get(),
                Some(threshold),
                "nested predicate descent/unwind not reached"
            );
            assert_eq!(
                error.span,
                Span {
                    start: threshold,
                    end: threshold
                }
            );
            usage(&session, &database, 0);
            drop(parse(&mut parser, b"0=0").unwrap());
        }
    }
}

#[test]
fn predicate_shared_database() {
    let retained = 35 + 32 * node_bytes();
    let peak = 35 + 48 * node_bytes();
    let database = Quota::new(2 * peak);
    let start = Barrier::new(3);
    let outputs = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..2 {
            let database = &database;
            let start = &start;
            handles.push(scope.spawn(move || {
                let session = Quota::new(SESSION);
                let mut parser = Parser::new(&session, database);
                start.wait();
                let result = parse(&mut parser, &chain(5, "AND"));
                (session, result)
            }));
        }
        start.wait();
        handles
            .into_iter()
            .map(|handle| handle.join())
            .collect::<Vec<_>>()
    });
    let held = outputs
        .into_iter()
        .map(|output| output.unwrap())
        .collect::<Vec<_>>();
    for (session, result) in &held {
        assert!(
            result.is_ok(),
            "both independent callers must succeed: {result:?}"
        );
        assert_eq!(session.used(), retained);
    }
    assert_eq!(database.used(), 2 * retained);
    for (session, result) in held {
        drop(result);
        assert_eq!(session.used(), 0);
    }
    assert_eq!(database.used(), 0);

    let database = Quota::new(retained + peak - 1);
    let (first_session, first) = std::thread::scope(|scope| {
        scope
            .spawn(|| {
                let session = Quota::new(SESSION);
                let mut parser = Parser::new(&session, &database);
                let result = parse(&mut parser, &chain(5, "AND"));
                (session, result)
            })
            .join()
            .unwrap()
    });
    let first = first.unwrap();
    let (second_session, second) = std::thread::scope(|scope| {
        scope
            .spawn(|| {
                let session = Quota::new(SESSION);
                let mut parser = Parser::new(&session, &database);
                let result = parse(&mut parser, &chain(5, "AND"));
                (session, result)
            })
            .join()
            .unwrap()
    });
    assert_eq!(second.unwrap_err(), limit(32));
    assert_eq!(database.used(), retained);
    assert_eq!(first_session.used(), retained);
    assert_eq!(second_session.used(), 0);
    drop(first);
    assert_eq!(database.used(), 0);
    assert_eq!(first_session.used(), 0);
}

#[test]
fn predicate_bounded_stack_and_drop() {
    assert!(size_of::<Parser>() <= 128);
    assert!(size_of::<ParsedStatement>() <= 384);
    assert!(size_of::<AstNode>() <= 256);
    std::thread::Builder::new()
        .stack_size(256 << 10)
        .spawn(|| {
            let session = Quota::new(SESSION);
            let database = Quota::new(DATABASE);
            let mut parser = Parser::new(&session, &database);
            for depth in [63, 64, 65] {
                for groups in [false, true] {
                    let source = if groups {
                        format!("{}0=0{}", "(".repeat(depth), ")".repeat(depth))
                    } else {
                        format!("{}0=0", "NOT ".repeat(depth))
                    };
                    let result = parse(&mut parser, source.as_bytes());
                    if depth == 65 {
                        assert_eq!(result.unwrap_err().code, Code::Limit);
                    } else {
                        drop(result.unwrap());
                    }
                    usage(&session, &database, 0);
                }
            }
            for depth in [63, 64, 65] {
                let groups = depth - 32;
                let source = format!(
                    "{}{}0=0{}",
                    "NOT ".repeat(32),
                    "(".repeat(groups),
                    ")".repeat(groups)
                );
                let result = parse(&mut parser, source.as_bytes());
                if depth == 65 {
                    assert_eq!(result.unwrap_err().code, Code::Limit);
                } else {
                    drop(result.unwrap());
                }
                usage(&session, &database, 0);
            }
            let group = format!("{}0=0{}", "(".repeat(64), ")".repeat(64));
            drop(parse(&mut parser, format!("{group} OR {group}").as_bytes()).unwrap());
            for nots in [2, 3] {
                let mut input = "NOT ".repeat(nots).into_bytes();
                input.extend_from_slice(&chain(2729, "AND"));
                let result = parse(&mut parser, &input);
                if nots == 2 {
                    let tree = result.unwrap();
                    assert_eq!(tree.nodes().len(), 8192);
                    drop(tree);
                } else {
                    assert_eq!(result.unwrap_err().code, Code::Limit);
                }
                usage(&session, &database, 0);
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn predicate_nested_failures_and_reuse() {
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut parser = Parser::new(&session, &database);
    let old = parse(&mut parser, b"0=0").unwrap();
    let held = charge(&old);
    let bad_name = format!("(\"{}\" = 1)", "n".repeat(129)).into_bytes();
    for (input, code) in [
        (b"((a = 1) AND (b = ))".to_vec(), Code::Syntax),
        (bad_name, Code::Name),
        (b"(a != 1)".to_vec(), Code::Unsupported),
        (vec![0xff], Code::Syntax),
    ] {
        assert_eq!(parse(&mut parser, &input).unwrap_err().code, code);
        usage(&session, &database, held);
        assert_eq!(old.source(), b"0=0");
        drop(parse(&mut parser, b"0=0").unwrap());
        usage(&session, &database, held);
    }
    drop(old);
    usage(&session, &database, 0);
}
