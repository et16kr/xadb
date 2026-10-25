//! Consumes the independently frozen dev4-splitter/control.json inventory.
#![cfg(feature = "test-support")]

use foundation::runtime::{Cancellation, Clock, Quota};
use sql::frontend::{
    CallControl, Code, Diagnostic, Module, Span, SplitOutcome, SplitStep, Splitter, StatementSource,
};
use std::{cell::Cell, sync::Barrier};

const SESSION: u64 = 8 << 20;
const DATABASE: u64 = 64 << 20;

#[derive(Default)]
struct TestClock(Cell<u64>);
impl Clock for TestClock {
    fn now_nanos(&self) -> u64 {
        self.0.get()
    }
}

fn push(splitter: &mut Splitter, bytes: &[u8]) -> Result<SplitStep, Diagnostic> {
    splitter.push(
        bytes,
        &CallControl::new(&Cancellation::default(), &TestClock::default(), None),
    )
}

fn finish(splitter: &mut Splitter) -> Result<Option<StatementSource>, Diagnostic> {
    splitter.finish(&CallControl::new(
        &Cancellation::default(),
        &TestClock::default(),
        None,
    ))
}

fn source(step: SplitStep) -> StatementSource {
    match step.outcome {
        SplitOutcome::Statement(value) => value,
        SplitOutcome::NeedMore => panic!("expected source"),
    }
}

fn pending(step: SplitStep, consumed: usize) {
    assert_eq!(step.consumed, consumed);
    assert!(matches!(step.outcome, SplitOutcome::NeedMore));
}

fn error(code: Code, position: usize) -> Diagnostic {
    Diagnostic {
        code,
        module: Module::Split,
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

fn sticky(splitter: &mut Splitter, expected: Diagnostic) {
    assert_eq!(push(splitter, b"ignored").unwrap_err(), expected);
    assert_eq!(finish(splitter).unwrap_err(), expected);
}

#[test]
fn capacity_and_retained_source_lifetimes() {
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut splitter = Splitter::new(&session, &database);
    assert_eq!(splitter.buffer_capacity(), 0);
    usage(&session, &database, 0);
    let first = source(push(&mut splitter, b"a;").unwrap());
    let c1 = first.capacity_bytes() as u64;
    assert!(c1 >= 256);
    assert_eq!(first.as_bytes(), b"a;");
    assert_eq!(first.span(), Span { start: 0, end: 2 });
    assert!(first.terminated());
    assert_eq!(splitter.buffer_capacity(), 0);
    usage(&session, &database, c1);
    let second = source(push(&mut splitter, b"b;").unwrap());
    let c2 = second.capacity_bytes() as u64;
    assert_eq!(second.as_bytes(), b"b;");
    assert_eq!(second.span(), Span { start: 2, end: 4 });
    usage(&session, &database, c1 + c2);
    drop(first);
    usage(&session, &database, c2);
    drop(splitter);
    usage(&session, &database, c2);
    drop(second);
    usage(&session, &database, 0);
    eprintln!("initial actual capacities: {c1}, {c2}");
}

#[test]
fn initial_reservation_boundaries() {
    for local in [true, false] {
        for limit in [255, 256, 257] {
            let session = Quota::new(if local { limit } else { SESSION });
            let database = Quota::new(if local { DATABASE } else { limit });
            let mut splitter = Splitter::new(&session, &database);
            let result = push(&mut splitter, b"a");
            if limit == 255 {
                assert_eq!(result.unwrap_err(), error(Code::Limit, 0));
                usage(&session, &database, 0);
            } else {
                pending(result.unwrap(), 1);
                assert!(splitter.buffer_capacity() >= 256);
                usage(&session, &database, splitter.buffer_capacity() as u64);
            }
            drop(splitter);
            usage(&session, &database, 0);
        }
    }
}

#[test]
fn growth_reservation_boundaries() {
    for local in [true, false] {
        for limit in [767, 768, 769] {
            let session = Quota::new(if local { limit } else { SESSION });
            let database = Quota::new(if local { DATABASE } else { limit });
            let mut splitter = Splitter::new(&session, &database);
            pending(push(&mut splitter, &[b'a'; 256]).unwrap(), 256);
            assert_eq!(splitter.buffer_capacity(), 256);
            let witnessed = Cell::new(false);
            let observer = |module, _| {
                if module == Module::Split && database.used() == 768 {
                    witnessed.set(true);
                    assert_eq!(session.used(), 768);
                }
            };
            let token = Cancellation::default();
            let clock = TestClock::default();
            let control = CallControl::new(&token, &clock, None).with_observer(&observer);
            let result = splitter.push(b"b", &control);
            if limit == 767 {
                assert_eq!(result.unwrap_err(), error(Code::Limit, 256));
                usage(&session, &database, 0);
            } else {
                pending(result.unwrap(), 1);
                assert!(witnessed.get());
                assert_eq!(splitter.buffer_capacity(), 512);
                usage(&session, &database, 512);
                let output = finish(&mut splitter).unwrap().unwrap();
                assert_eq!(&output.as_bytes()[..256], &[b'a'; 256]);
                assert_eq!(output.as_bytes()[256], b'b');
                drop(output);
            }
            drop(splitter);
            usage(&session, &database, 0);
        }
    }
}

#[test]
fn retained_source_preserved_after_next_failure() {
    for local in [true, false] {
        for remaining in [255, 256, 257] {
            let session = Quota::new(if local { 256 + remaining } else { SESSION });
            let database = Quota::new(if local { DATABASE } else { 256 + remaining });
            let mut splitter = Splitter::new(&session, &database);
            let prior = source(push(&mut splitter, b"a;").unwrap());
            assert_eq!(prior.capacity_bytes(), 256);
            let result = push(&mut splitter, b"b");
            if remaining == 255 {
                assert_eq!(result.unwrap_err(), error(Code::Limit, 2));
            } else {
                pending(result.unwrap(), 1);
                usage(&session, &database, 512);
            }
            drop(splitter);
            assert_eq!(prior.as_bytes(), b"a;");
            usage(&session, &database, prior.capacity_bytes() as u64);
            drop(prior);
            usage(&session, &database, 0);
        }
    }
}

#[test]
fn database_failure_releases_prior_session_reservation() {
    let session = Quota::new(SESSION);
    let database = Quota::new(255);
    let mut splitter = Splitter::new(&session, &database);
    let failure = push(&mut splitter, b"a").unwrap_err();
    assert_eq!(failure, error(Code::Limit, 0));
    assert_eq!(session.peak(), 256);
    assert_eq!(database.peak(), 0);
    usage(&session, &database, 0);
    sticky(&mut splitter, failure);
    let adequate = Quota::new(DATABASE);
    let output = source(push(&mut Splitter::new(&session, &adequate), b"a;").unwrap());
    assert_eq!(output.as_bytes(), b"a;");
    drop(output);
    usage(&session, &adequate, 0);
}

#[test]
fn actual_capacity_padding_is_charged() {
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut splitter = Splitter::new(&session, &database);
    splitter.set_allocation_padding(16);
    pending(push(&mut splitter, b"a").unwrap(), 1);
    let old = splitter.buffer_capacity();
    assert!(old >= 272);
    usage(&session, &database, old as u64);
    pending(push(&mut splitter, &vec![b'a'; old]).unwrap(), old);
    let new = splitter.buffer_capacity();
    assert!(new >= old * 2 + 16);
    usage(&session, &database, new as u64);
    assert_eq!(session.peak(), (old + new) as u64);
    assert_eq!(database.peak(), (old + new) as u64);
    let output = finish(&mut splitter).unwrap().unwrap();
    assert_eq!(output.capacity_bytes(), new);
    assert_eq!(output.as_bytes(), vec![b'a'; old + 1]);
    drop(output);
    usage(&session, &database, 0);
    eprintln!("padded actual capacities: {old}, {new}; peak {}", old + new);
}

fn padding_failure(local: bool) {
    let session = Quota::new(if local { 256 } else { SESSION });
    let database = Quota::new(if local { DATABASE } else { 256 });
    let mut splitter = Splitter::new(&session, &database);
    splitter.set_allocation_padding(16);
    assert_eq!(
        push(&mut splitter, b"a").unwrap_err(),
        error(Code::Limit, 0)
    );
    assert_eq!(database.peak(), 256);
    assert_eq!(session.peak(), if local { 256 } else { 272 });
    usage(&session, &database, 0);
    assert_eq!(splitter.buffer_capacity(), 0);
}

#[test]
fn padding_extra_session_failure_cleans_all() {
    padding_failure(true);
}

#[test]
fn padding_extra_database_failure_cleans_all() {
    padding_failure(false);
}

#[test]
fn allocation_failure_before_first_backing() {
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut splitter = Splitter::new(&session, &database);
    splitter.fail_next_allocation();
    let failure = push(&mut splitter, b"a").unwrap_err();
    assert_eq!(failure, error(Code::Limit, 0));
    usage(&session, &database, 0);
    assert_eq!(session.peak(), 256);
    assert_eq!(database.peak(), 256);
    sticky(&mut splitter, failure);
    let output = source(push(&mut Splitter::new(&session, &database), b"a;").unwrap());
    assert_eq!(output.as_bytes(), b"a;");
    drop(output);
    usage(&session, &database, 0);
}

#[test]
fn allocation_failure_during_growth() {
    for retain in [false, true] {
        let session = Quota::new(SESSION);
        let database = Quota::new(DATABASE);
        let mut splitter = Splitter::new(&session, &database);
        let prior = retain.then(|| source(push(&mut splitter, b"a;").unwrap()));
        let baseline = prior.as_ref().map_or(0, |s| s.capacity_bytes() as u64);
        pending(push(&mut splitter, &[b'a'; 256]).unwrap(), 256);
        splitter.fail_next_allocation();
        let position = if retain { 258 } else { 256 };
        let failure = push(&mut splitter, b"b").unwrap_err();
        assert_eq!(failure, error(Code::Limit, position));
        assert_eq!(session.peak(), baseline + 768);
        assert_eq!(database.peak(), baseline + 768);
        usage(&session, &database, baseline);
        assert_eq!(splitter.buffer_capacity(), 0);
        sticky(&mut splitter, failure);
        if let Some(output) = &prior {
            assert_eq!(output.as_bytes(), b"a;");
        }
        drop(prior);
        usage(&session, &database, 0);
    }
}

#[test]
fn scan_control_is_bounded_and_terminal() {
    for prefix in [b"".as_slice(), b"'", b"/*"] {
        for timeout in [false, true] {
            let session = Quota::new(SESSION);
            let database = Quota::new(DATABASE);
            let mut splitter = Splitter::new(&session, &database);
            let mut input = prefix.to_vec();
            input.resize(600, b'a');
            pending(push(&mut splitter, &input).unwrap(), 600);
            assert_eq!(splitter.buffer_capacity(), 1024);
            let token = Cancellation::default();
            let clock = TestClock::default();
            let observer = |module, span: Span| {
                if module == Module::Split && span.start >= 700 && database.used() == 1024 {
                    if timeout {
                        clock.0.set(9);
                    } else {
                        token.cancel();
                    }
                }
            };
            let control = CallControl::new(&token, &clock, Some(9)).with_observer(&observer);
            let failure = splitter.push(&vec![b'a'; 100_000], &control).unwrap_err();
            assert!((700..=856).contains(&failure.span.start), "{failure:?}");
            assert_eq!(
                failure,
                error(
                    if timeout {
                        Code::Timeout
                    } else {
                        Code::Cancelled
                    },
                    failure.span.start
                )
            );
            usage(&session, &database, 0);
            sticky(&mut splitter, failure);
            let output = source(push(&mut Splitter::new(&session, &database), b"a;").unwrap());
            assert_eq!(output.as_bytes(), b"a;");
            drop(output);
            usage(&session, &database, 0);
        }
    }
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut splitter = Splitter::new(&session, &database);
    let token = Cancellation::default();
    token.cancel();
    let clock = TestClock::default();
    let failure = splitter
        .push(b"\xff", &CallControl::new(&token, &clock, Some(0)))
        .unwrap_err();
    assert_eq!(failure, error(Code::Cancelled, 0));
    usage(&session, &database, 0);
}

#[test]
fn copy_control_is_bounded_and_cleans_old_new() {
    for timeout in [false, true] {
        let session = Quota::new(SESSION);
        let database = Quota::new(DATABASE);
        let mut splitter = Splitter::new(&session, &database);
        pending(push(&mut splitter, &[b'a'; 512]).unwrap(), 512);
        assert_eq!(splitter.buffer_capacity(), 512);
        let token = Cancellation::default();
        let clock = TestClock::default();
        let witnessed = Cell::new(false);
        let observer = |module, span: Span| {
            if module == Module::Split
                && database.used() == 1536
                && (256..512).contains(&span.start)
            {
                assert_eq!(session.used(), 1536);
                witnessed.set(true);
                if timeout {
                    clock.0.set(9);
                } else {
                    token.cancel();
                }
            }
        };
        let control = CallControl::new(&token, &clock, Some(9)).with_observer(&observer);
        let failure = splitter.push(b"b", &control).unwrap_err();
        assert!(witnessed.get());
        assert_eq!(
            failure,
            error(
                if timeout {
                    Code::Timeout
                } else {
                    Code::Cancelled
                },
                256
            )
        );
        usage(&session, &database, 0);
        assert_eq!(splitter.buffer_capacity(), 0);
        sticky(&mut splitter, failure);
    }
}

#[test]
fn new_calls_do_not_retain_previous_control() {
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut splitter = Splitter::new(&session, &database);
    let clock = TestClock::default();
    let first = Cancellation::default();
    pending(
        splitter
            .push(b"a", &CallControl::new(&first, &clock, None))
            .unwrap(),
        1,
    );
    first.cancel();
    let second = Cancellation::default();
    let output = source(
        splitter
            .push(b";", &CallControl::new(&second, &clock, None))
            .unwrap(),
    );
    second.cancel();
    assert!(finish(&mut splitter).unwrap().is_none());
    assert_eq!(output.as_bytes(), b"a;");
    usage(&session, &database, output.capacity_bytes() as u64);
    drop(output);
    usage(&session, &database, 0);
}

#[test]
fn finished_and_failed_precedence() {
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let token = Cancellation::default();
    token.cancel();
    let clock = TestClock::default();
    let cancelled = CallControl::new(&token, &clock, Some(0));
    let mut first = Splitter::new(&session, &database);
    assert!(finish(&mut first).unwrap().is_none());
    let failure = first.finish(&cancelled).unwrap_err();
    assert_eq!(failure, error(Code::Cancelled, 0));
    sticky(&mut first, failure);
    let mut second = Splitter::new(&session, &database);
    assert!(finish(&mut second).unwrap().is_none());
    assert_eq!(push(&mut second, b"").unwrap_err(), error(Code::Syntax, 0));
    let mut third = Splitter::new(&session, &database);
    let failure = push(&mut third, b"\xff").unwrap_err();
    assert_eq!(failure.code, Code::Syntax);
    assert_eq!(third.push(b"a", &cancelled).unwrap_err(), failure);
    assert_eq!(third.finish(&cancelled).unwrap_err(), failure);
    usage(&session, &database, 0);
}

#[test]
fn shared_database_two_splitters_are_bounded() {
    for limit in [511, 512, 513] {
        let session1 = Quota::new(SESSION);
        let session2 = Quota::new(SESSION);
        let database = Quota::new(limit);
        let mut first = Splitter::new(&session1, &database);
        let mut second = Splitter::new(&session2, &database);
        let prior = source(push(&mut first, b"a;").unwrap());
        let result = push(&mut second, b"b");
        if limit == 511 {
            assert_eq!(result.unwrap_err(), error(Code::Limit, 0));
            assert_eq!(session2.used(), 0);
        } else {
            pending(result.unwrap(), 1);
            assert_eq!(session2.used(), second.buffer_capacity() as u64);
        }
        assert_eq!(database.used(), session1.used() + session2.used());
        assert!(database.used() <= limit);
        assert_eq!(prior.as_bytes(), b"a;");
        drop(second);
        assert_eq!(database.used(), prior.capacity_bytes() as u64);
        drop(prior);
        usage(&session1, &database, 0);
    }
}

#[test]
fn native_threads_share_database_quota() {
    let database = Quota::new(256);
    let barrier = Barrier::new(2);
    let wins = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..2)
            .map(|_| {
                scope.spawn(|| {
                    let session = Quota::new(SESSION);
                    let mut splitter = Splitter::new(&session, &database);
                    barrier.wait();
                    let result = push(&mut splitter, b"a");
                    let won = result.is_ok();
                    barrier.wait();
                    let observed_total = database.used();
                    barrier.wait();
                    assert_eq!(observed_total, 256);
                    if let Err(failure) = result {
                        assert_eq!(failure, error(Code::Limit, 0));
                        assert_eq!(session.used(), 0);
                    }
                    if won {
                        let output = finish(&mut splitter).unwrap().unwrap();
                        assert_eq!(output.as_bytes(), b"a");
                        assert_eq!(output.capacity_bytes(), 256);
                        drop(output);
                    }
                    drop(splitter);
                    assert_eq!(session.used(), 0);
                    usize::from(won)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .sum::<usize>()
    });
    assert_eq!(wins, 1);
    assert_eq!(database.used(), 0);
    assert_eq!(database.peak(), 256);
}

#[test]
fn empty_error_and_drop_release_pending_capacity() {
    for (prefix, suffix, fails) in [
        (b" ".as_slice(), b";".as_slice(), false),
        (b"/*", b"*/", false),
        (b"'", b"", true),
        (b"/*", b"", true),
        (b" ", b"\xf0\x9f", true),
    ] {
        let session = Quota::new(SESSION);
        let database = Quota::new(DATABASE);
        let mut splitter = Splitter::new(&session, &database);
        let mut input = prefix.to_vec();
        input.resize(600, b' ');
        input.extend_from_slice(suffix);
        pending(push(&mut splitter, &input).unwrap(), input.len());
        if fails {
            let failure = finish(&mut splitter).unwrap_err();
            assert_eq!(failure.code, Code::Syntax);
            assert_eq!(failure.module, Module::Split);
            assert_eq!(failure.span.end, input.len());
        } else {
            assert!(finish(&mut splitter).unwrap().is_none());
        }
        usage(&session, &database, 0);
        assert_eq!(splitter.buffer_capacity(), 0);
    }
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut splitter = Splitter::new(&session, &database);
    pending(push(&mut splitter, &[b'a'; 600]).unwrap(), 600);
    assert!(session.used() >= 600);
    drop(splitter);
    usage(&session, &database, 0);
}

#[test]
fn splitter_stack_footprints_are_bounded() {
    assert!(std::mem::size_of::<Splitter>() <= 512);
    assert!(std::mem::size_of::<StatementSource>() <= 192);
}
