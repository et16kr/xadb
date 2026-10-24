//! P3 scalar resource inputs; frozen before product implementation.
#![cfg(feature = "test-support")]

use std::{cell::Cell, mem::size_of, sync::Barrier};
use xadb_foundation::runtime::{Cancellation, Clock, Quota};
use xadb_sql::frontend::{
    CallControl, Code, DeclaredType, DecodedScalar, Diagnostic, Module, NodeKind, OwnedScalar,
    ParsedStatement, Parser, ScalarReader, ScalarRef, ScalarSpec, ScalarType, Span,
};

const SESSION: u64 = 8 << 20;
const DATABASE: u64 = 64 << 20;
const AT: Span = Span { start: 7, end: 9 };

#[derive(Default)]
struct TestClock(Cell<u64>);
impl Clock for TestClock {
    fn now_nanos(&self) -> u64 {
        self.0.get()
    }
}
fn spec(text: bool) -> ScalarSpec {
    ScalarSpec {
        declared_type: if text {
            DeclaredType::Utf8(65536)
        } else {
            DeclaredType::Binary(65536)
        },
        nullable: true,
    }
}
fn fixture(raw: &str) -> (ParsedStatement, usize) {
    fixture_on(raw, &Quota::new(SESSION), &Quota::new(DATABASE))
}
fn fixture_on(raw: &str, session: &Quota, database: &Quota) -> (ParsedStatement, usize) {
    let source = format!("INSERT INTO t(a) VALUES({raw})");
    let mut parser = Parser::new(session, database);
    let statement = parser
        .parse(
            source.as_bytes(),
            &CallControl::new(&Cancellation::default(), &TestClock::default(), None),
        )
        .unwrap();
    let index = statement
        .nodes()
        .iter()
        .position(|n| matches!(n.kind(), NodeKind::Scalar(_)))
        .unwrap();
    (statement, index)
}
fn literal(decoded: DecodedScalar) -> OwnedScalar {
    match decoded {
        DecodedScalar::Value(v) => v,
        _ => panic!("expected typed value"),
    }
}
fn decode(
    reader: &mut ScalarReader,
    statement: &ParsedStatement,
    index: usize,
) -> Result<DecodedScalar, Diagnostic> {
    reader.decode(
        statement,
        index,
        &CallControl::new(&Cancellation::default(), &TestClock::default(), None),
    )
}
fn copy(
    reader: &mut ScalarReader,
    input: ScalarRef<'_>,
    text: bool,
    native: bool,
) -> Result<OwnedScalar, Diagnostic> {
    let cancel = Cancellation::default();
    let clock = TestClock::default();
    let control = CallControl::new(&cancel, &clock, None);
    if native {
        reader.copy_native(input, spec(text), AT, &control)
    } else {
        reader.copy_sql(input, spec(text), AT, &control)
    }
}
fn usage(session: &Quota, database: &Quota, n: u64) {
    assert_eq!(session.used(), n);
    assert_eq!(database.used(), n);
}
fn error(code: Code, span: Span) -> Diagnostic {
    Diagnostic {
        code,
        module: Module::Value,
        span,
    }
}
fn point(start: usize) -> Span {
    Span { start, end: start }
}
struct Inputs {
    text: (ParsedStatement, usize),
    binary: (ParsedStatement, usize),
    bytes: Vec<u8>,
    utf8: String,
}
impl Inputs {
    fn new(n: usize) -> Self {
        Self {
            text: fixture(&format!("'{}'", "a".repeat(n))),
            binary: fixture(&format!("X'{}'", "01".repeat(n))),
            bytes: vec![1; n],
            utf8: "a".repeat(n),
        }
    }
    fn call(&self, path: usize, reader: &mut ScalarReader) -> Result<OwnedScalar, Diagnostic> {
        match path {
            0 => decode(reader, &self.text.0, self.text.1).map(literal),
            1 => decode(reader, &self.binary.0, self.binary.1).map(literal),
            2 => copy(reader, ScalarRef::Utf8(&self.utf8), true, false),
            3 => copy(reader, ScalarRef::Binary(&self.bytes), false, true),
            _ => unreachable!(),
        }
    }
    fn location(&self, path: usize) -> Span {
        match path {
            0 => point(self.text.0.nodes()[self.text.1].span().start),
            1 => point(self.binary.0.nodes()[self.binary.1].span().start),
            _ => AT,
        }
    }
}

#[test]
fn retained_outputs_charge_actual_backing() {
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let (statement, index) = fixture_on("'a''가'", &session, &database);
    let parsed = (statement.source_capacity_bytes() + statement.arena_capacity_bytes()) as u64;
    let mut reader = ScalarReader::new(&session, &database);
    let first = literal(decode(&mut reader, &statement, index).unwrap());
    assert_eq!(first.as_ref(), ScalarRef::Utf8("a'가"));
    assert_eq!(first.capacity_bytes(), 5);
    usage(&session, &database, parsed + 5);
    let mut source = "xyz".to_string();
    let second = copy(&mut reader, ScalarRef::Utf8(&source), true, true).unwrap();
    source.clear();
    source.push_str("bad");
    drop(source);
    usage(&session, &database, parsed + 8);
    drop(statement);
    drop(reader);
    usage(&session, &database, 8);
    assert_eq!(second.as_ref(), ScalarRef::Utf8("xyz"));
    drop(first);
    usage(&session, &database, 3);
    drop(second);
    usage(&session, &database, 0);
    let (statement, index) = fixture_on("X'00ff'", &session, &database);
    let mut reader = ScalarReader::new(&session, &database);
    let binary = literal(decode(&mut reader, &statement, index).unwrap());
    drop(statement);
    usage(&session, &database, 2);
    assert_eq!(binary.as_ref(), ScalarRef::Binary(&[0, 255]));
    let mut bytes = vec![1, 2, 3];
    let native = copy(&mut reader, ScalarRef::Binary(&bytes), false, true).unwrap();
    bytes.fill(9);
    drop(bytes);
    let mut input = "sql".to_string();
    let sql = copy(&mut reader, ScalarRef::Utf8(&input), true, false).unwrap();
    input.clear();
    drop(input);
    drop(reader);
    usage(&session, &database, 8);
    assert_eq!(native.as_ref(), ScalarRef::Binary(&[1, 2, 3]));
    assert_eq!(sql.as_ref(), ScalarRef::Utf8("sql"));
    drop(binary);
    usage(&session, &database, 6);
    drop(native);
    usage(&session, &database, 3);
    drop(sql);
    usage(&session, &database, 0);
}

#[test]
fn base_reservation_boundaries() {
    let inputs = Inputs::new(4096);
    for path in 0..4 {
        for local in [true, false] {
            for limit in [4095, 4096, 4097] {
                let session = Quota::new(if local { limit } else { SESSION });
                let database = Quota::new(if local { DATABASE } else { limit });
                let mut reader = ScalarReader::new(&session, &database);
                let result = inputs.call(path, &mut reader);
                if limit < 4096 {
                    assert_eq!(
                        result.unwrap_err(),
                        error(Code::Limit, inputs.location(path))
                    );
                    assert_eq!(reader.allocation_count(), 0);
                } else {
                    let value = result.unwrap();
                    assert_eq!(value.capacity_bytes(), 4096);
                    usage(&session, &database, 4096);
                    assert_eq!(reader.allocation_count(), 1);
                    drop(value);
                }
                usage(&session, &database, 0);
                assert_eq!(session.peak(), if local && limit < 4096 { 0 } else { 4096 });
                assert_eq!(database.peak(), if limit < 4096 { 0 } else { 4096 });
            }
        }
    }
}

#[test]
fn actual_capacity_and_excess_failure() {
    let inputs = Inputs::new(4096);
    for path in 0..4 {
        for local in [true, false] {
            for limit in [4112, 4113, 4114] {
                let session = Quota::new(if local { limit } else { SESSION });
                let database = Quota::new(if local { DATABASE } else { limit });
                let mut reader = ScalarReader::new(&session, &database);
                reader.set_allocation_padding(17);
                let result = inputs.call(path, &mut reader);
                assert_eq!(reader.allocation_count(), 1);
                if limit < 4113 {
                    assert_eq!(
                        result.unwrap_err(),
                        error(Code::Limit, inputs.location(path))
                    );
                } else {
                    let value = result.unwrap();
                    assert_eq!(value.capacity_bytes(), 4113);
                    usage(&session, &database, 4113);
                    drop(value);
                }
                usage(&session, &database, 0);
                assert_eq!(
                    session.peak(),
                    if local && limit < 4113 { 4096 } else { 4113 }
                );
                assert_eq!(database.peak(), if limit < 4113 { 4096 } else { 4113 });
                if limit >= 4113 {
                    let padded = inputs.call(path, &mut reader).unwrap();
                    assert_eq!(padded.capacity_bytes(), 4113);
                    drop(padded);
                    reader.set_allocation_padding(0);
                    let exact = inputs.call(path, &mut reader).unwrap();
                    assert_eq!(exact.capacity_bytes(), 4096);
                    drop(exact);
                    usage(&session, &database, 0);
                }
            }
        }
    }
}

#[test]
fn all_allocation_failures_and_reuse() {
    let inputs = Inputs::new(4096);
    for path in 0..4 {
        let session = Quota::new(SESSION);
        let database = Quota::new(DATABASE);
        let mut reader = ScalarReader::new(&session, &database);
        let retained = copy(&mut reader, ScalarRef::Utf8("old"), true, false).unwrap();
        reader.fail_allocation_at(1);
        assert_eq!(
            inputs.call(path, &mut reader).unwrap_err(),
            error(Code::Limit, inputs.location(path))
        );
        assert_eq!(reader.allocation_count(), 1);
        usage(&session, &database, 3);
        assert_eq!(retained.as_ref(), ScalarRef::Utf8("old"));
        drop(inputs.call(path, &mut reader).unwrap());
        usage(&session, &database, 3);
        reader.fail_allocation_at(2);
        drop(inputs.call(path, &mut reader).unwrap());
        drop(inputs.call(path, &mut reader).unwrap());
        reader.fail_allocation_at(1);
        reader.fail_allocation_at(0);
        drop(inputs.call(path, &mut reader).unwrap());
        drop(retained);
        usage(&session, &database, 0);
    }
}

#[test]
fn zero_payload_and_fixed_values_allocate_nothing() {
    let session = Quota::new(0);
    let database = Quota::new(0);
    let mut reader = ScalarReader::new(&session, &database);
    reader.set_allocation_padding(usize::MAX);
    for raw in [
        "0",
        "-9223372036854775808",
        "TRUE",
        "FALSE",
        "NULL",
        "?",
        "''",
        "X''",
    ] {
        let (statement, index) = fixture(raw);
        reader.fail_allocation_at(1);
        drop(decode(&mut reader, &statement, index).unwrap());
        assert_eq!(reader.allocation_count(), 0);
        usage(&session, &database, 0);
    }
    for native in [false, true] {
        for input in [
            ScalarRef::Utf8(""),
            ScalarRef::Binary(&[]),
            ScalarRef::Null(ScalarType::Utf8),
        ] {
            reader.fail_allocation_at(1);
            drop(
                copy(
                    &mut reader,
                    input,
                    !matches!(input, ScalarRef::Binary(_)),
                    native,
                )
                .unwrap(),
            );
            assert_eq!(reader.allocation_count(), 0);
        }
    }
    // On a nonzero ledger, a zero-allocation call consumes the pending injection.
    let session = Quota::new(16);
    let database = Quota::new(16);
    let mut reader = ScalarReader::new(&session, &database);
    reader.fail_allocation_at(1);
    drop(copy(&mut reader, ScalarRef::Utf8(""), true, false).unwrap());
    drop(copy(&mut reader, ScalarRef::Utf8("ok"), true, false).unwrap());
    usage(&session, &database, 0);
}

#[test]
fn padding_overflow_returns_all_leases() {
    let inputs = Inputs::new(3);
    for path in 0..4 {
        let session = Quota::new(SESSION);
        let database = Quota::new(DATABASE);
        let mut reader = ScalarReader::new(&session, &database);
        reader.set_allocation_padding(usize::MAX);
        assert_eq!(
            inputs.call(path, &mut reader).unwrap_err(),
            error(Code::Limit, inputs.location(path))
        );
        assert_eq!(reader.allocation_count(), 1);
        usage(&session, &database, 0);
        reader.set_allocation_padding(0);
        drop(inputs.call(path, &mut reader).unwrap());
        usage(&session, &database, 0);
    }
}

#[test]
fn entry_control_and_reader_reuse() {
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut reader = ScalarReader::new(&session, &database);
    let (statement, index) = fixture("'ok'");
    let at = point(statement.nodes()[index].span().start);
    for cancelled in [true, false] {
        let cancel = Cancellation::default();
        if cancelled {
            cancel.cancel();
        }
        let clock = TestClock(Cell::new(9));
        reader.fail_allocation_at(1);
        assert_eq!(
            reader
                .decode(
                    &statement,
                    index,
                    &CallControl::new(&cancel, &clock, Some(9))
                )
                .unwrap_err(),
            error(
                if cancelled {
                    Code::Cancelled
                } else {
                    Code::Timeout
                },
                at
            )
        );
        assert_eq!(reader.allocation_count(), 0);
        usage(&session, &database, 0);
        drop(decode(&mut reader, &statement, index).unwrap());
        usage(&session, &database, 0);
        reader.fail_allocation_at(1);
        assert_eq!(
            reader
                .copy_sql(
                    ScalarRef::Utf8("ok"),
                    spec(true),
                    AT,
                    &CallControl::new(&cancel, &clock, Some(9))
                )
                .unwrap_err(),
            error(
                if cancelled {
                    Code::Cancelled
                } else {
                    Code::Timeout
                },
                AT
            )
        );
        drop(copy(&mut reader, ScalarRef::Utf8("ok"), true, false).unwrap());
        usage(&session, &database, 0);
        reader.fail_allocation_at(1);
        assert_eq!(
            reader
                .copy_native(
                    ScalarRef::Utf8("ok"),
                    spec(true),
                    AT,
                    &CallControl::new(&cancel, &clock, Some(9))
                )
                .unwrap_err(),
            error(
                if cancelled {
                    Code::Cancelled
                } else {
                    Code::Timeout
                },
                AT
            )
        );
        assert_eq!(reader.allocation_count(), 0);
        drop(copy(&mut reader, ScalarRef::Utf8("ok"), true, true).unwrap());
        usage(&session, &database, 0);
    }
}

fn decode_control(raw: &str, charged: u64) {
    let (statement, index) = fixture(raw);
    let start = statement.nodes()[index].span().start;
    for deadline in [false, true] {
        let session = Quota::new(SESSION);
        let database = Quota::new(DATABASE);
        let mut reader = ScalarReader::new(&session, &database);
        let cancel = Cancellation::default();
        let clock = TestClock::default();
        let seen = Cell::new(None);
        let observer = |module, span: Span| {
            if module == Module::Value
                && database.used() == charged
                && (start + 1024..=start + 2048).contains(&span.start)
                && seen.get().is_none()
            {
                assert_eq!(span.start, span.end);
                seen.set(Some(span));
                if deadline {
                    clock.0.set(9);
                } else {
                    cancel.cancel();
                }
            }
        };
        let control =
            CallControl::new(&cancel, &clock, deadline.then_some(9)).with_observer(&observer);
        let result = reader.decode(&statement, index, &control);
        let observed = seen
            .get()
            .expect("required interior pass checkpoint was not reached");
        assert_eq!(
            result.unwrap_err(),
            error(
                if deadline {
                    Code::Timeout
                } else {
                    Code::Cancelled
                },
                observed
            )
        );
        usage(&session, &database, 0);
        drop(decode(&mut reader, &statement, index).unwrap());
        usage(&session, &database, 0);
    }
}
#[test]
fn integer_loop_checks_controls() {
    for sign in ["", "+", "-"] {
        decode_control(&format!("{sign}{}1", "0".repeat(4096)), 0);
    }
}
#[test]
fn text_validation_and_decode_check_controls() {
    for raw in [
        format!("'{}'", "a".repeat(4096)),
        format!("'{}'", "가''".repeat(1024)),
    ] {
        for charged in [0, 4096] {
            decode_control(&raw, charged);
        }
    }
}
#[test]
fn binary_validation_and_decode_check_controls() {
    let raw = format!("X'{}'", "00fF".repeat(2048));
    for charged in [0, 4096] {
        decode_control(&raw, charged);
    }
}
#[test]
fn borrowed_copy_checks_controls() {
    let utf8 = "가a".repeat(1024);
    let binary = vec![0xff; 4096];
    for native in [false, true] {
        for text in [false, true] {
            for deadline in [false, true] {
                let session = Quota::new(SESSION);
                let database = Quota::new(DATABASE);
                let mut reader = ScalarReader::new(&session, &database);
                let cancel = Cancellation::default();
                let clock = TestClock::default();
                let witnessed = Cell::new(false);
                let observer = |module, span: Span| {
                    if module == Module::Value
                        && database.used() == 4096
                        && (1024..=2048).contains(&span.start)
                    {
                        assert_eq!(span.start, span.end);
                        witnessed.set(true);
                        if deadline {
                            clock.0.set(9);
                        } else {
                            cancel.cancel();
                        }
                    }
                };
                let control = CallControl::new(&cancel, &clock, deadline.then_some(9))
                    .with_observer(&observer);
                let input = if text {
                    ScalarRef::Utf8(&utf8)
                } else {
                    ScalarRef::Binary(&binary)
                };
                let result = if native {
                    reader.copy_native(input, spec(text), AT, &control)
                } else {
                    reader.copy_sql(input, spec(text), AT, &control)
                };
                assert!(
                    witnessed.get(),
                    "required interior copy checks were not reached"
                );
                assert_eq!(
                    result.unwrap_err(),
                    error(
                        if deadline {
                            Code::Timeout
                        } else {
                            Code::Cancelled
                        },
                        AT
                    )
                );
                usage(&session, &database, 0);
                drop(copy(&mut reader, input, text, native).unwrap());
                usage(&session, &database, 0);
            }
        }
    }
}

#[test]
fn two_callers_share_database_quota() {
    for limit in [8191, 8192] {
        let database = Quota::new(limit);
        let barrier = Barrier::new(2);
        let results = std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for _ in 0..2 {
                let database = &database;
                let barrier = &barrier;
                handles.push(scope.spawn(move || {
                    let session = Quota::new(8192);
                    let mut reader = ScalarReader::new(&session, database);
                    let bytes = vec![42; 4096];
                    barrier.wait();
                    (
                        copy(&mut reader, ScalarRef::Binary(&bytes), false, true),
                        session,
                    )
                }));
            }
            handles
                .into_iter()
                .map(|h| h.join().unwrap())
                .collect::<Vec<_>>()
        });
        let successes = results.iter().filter(|(r, _)| r.is_ok()).count();
        assert_eq!(successes, if limit == 8192 { 2 } else { 1 });
        assert_eq!(database.used(), 4096 * successes as u64);
        for (r, session) in &results {
            match r {
                Ok(v) => {
                    assert_eq!(v.capacity_bytes(), 4096);
                    assert_eq!(session.used(), 4096);
                }
                Err(e) => {
                    assert_eq!(*e, error(Code::Limit, AT));
                    assert_eq!(session.used(), 0);
                }
            }
        }
        drop(results);
        assert_eq!(database.used(), 0);
    }
}

#[test]
fn bounded_fixed_state_and_drop() {
    assert!(size_of::<ScalarReader>() <= 128);
    assert!(size_of::<OwnedScalar>() <= 256);
    assert!(size_of::<DecodedScalar>() <= 272);
    assert!(size_of::<ScalarRef<'_>>() <= 32);
    let text = fixture(&format!("'{}'", "a".repeat(65536)));
    let binary = fixture(&format!("X'{}'", "ab".repeat(65536)));
    let thread = std::thread::Builder::new()
        .stack_size(256 << 10)
        .spawn(move || {
            let session = Quota::new(SESSION);
            let database = Quota::new(DATABASE);
            let mut reader = ScalarReader::new(&session, &database);
            let a = literal(decode(&mut reader, &text.0, text.1).unwrap());
            let b = literal(decode(&mut reader, &binary.0, binary.1).unwrap());
            assert_eq!(a.capacity_bytes(), 65536);
            assert_eq!(b.capacity_bytes(), 65536);
            usage(&session, &database, 131072);
            drop((a, b, text, binary, reader));
            usage(&session, &database, 0);
        })
        .unwrap();
    thread.join().unwrap();
}

#[test]
fn checkpoint_distances_are_bounded() {
    for raw in [
        format!("{}1", "0".repeat(4096)),
        format!("'{}'", "a".repeat(4096)),
        format!("'{}'", "가''".repeat(1024)),
        format!("X'{}'", "00fF".repeat(2048)),
    ] {
        let (statement, index) = fixture(&raw);
        let span = statement.nodes()[index].span();
        let session = Quota::new(SESSION);
        let database = Quota::new(DATABASE);
        let mut reader = ScalarReader::new(&session, &database);
        let phase = Cell::new(false);
        let last = Cell::new(span.start);
        let count = Cell::new(0);
        let observer = |module, at: Span| {
            if module != Module::Value {
                return;
            }
            assert_eq!(at.start, at.end);
            let charged = database.used() != 0;
            if charged != phase.get() {
                assert!(charged, "only validation to copy phase transition");
                assert_eq!(
                    last.get(),
                    span.end,
                    "validation reaches full raw input before allocation"
                );
                phase.set(charged);
                last.set(span.start);
            }
            assert!(
                at.start >= last.get(),
                "progress moves forward inside a pass"
            );
            assert!(
                at.start - last.get() <= 256,
                "long raw-input loop without control check"
            );
            assert!(at.start <= span.end);
            last.set(at.start);
            count.set(count.get() + 1);
        };
        let cancel = Cancellation::default();
        let clock = TestClock::default();
        let output = reader
            .decode(
                &statement,
                index,
                &CallControl::new(&cancel, &clock, None).with_observer(&observer),
            )
            .unwrap();
        assert_eq!(last.get(), span.end);
        assert!(count.get() > 16);
        drop(output);
        usage(&session, &database, 0);
    }
    let text = "가a".repeat(1024);
    let bytes = vec![0; 4096];
    for native in [false, true] {
        for utf8 in [false, true] {
            let session = Quota::new(SESSION);
            let database = Quota::new(DATABASE);
            let mut reader = ScalarReader::new(&session, &database);
            let last = Cell::new(0);
            let count = Cell::new(0);
            let observer = |module, at: Span| {
                if module == Module::Value {
                    assert_eq!(at.start, at.end);
                    assert!(at.start >= last.get());
                    assert!(at.start - last.get() <= 256);
                    last.set(at.start);
                    count.set(count.get() + 1);
                }
            };
            let cancel = Cancellation::default();
            let clock = TestClock::default();
            let control = CallControl::new(&cancel, &clock, None).with_observer(&observer);
            let input = if utf8 {
                ScalarRef::Utf8(&text)
            } else {
                ScalarRef::Binary(&bytes)
            };
            let result = if native {
                reader.copy_native(input, spec(utf8), AT, &control)
            } else {
                reader.copy_sql(input, spec(utf8), AT, &control)
            };
            drop(result.unwrap());
            assert_eq!(last.get(), 4096);
            assert!(count.get() >= 17);
            usage(&session, &database, 0);
        }
    }
}

#[test]
fn retained_output_reservation_boundaries() {
    let inputs = Inputs::new(4096);
    for path in 0..4 {
        for local in [false, true] {
            for limit in [4098, 4099, 4100] {
                let session = Quota::new(if local { limit } else { SESSION });
                let database = Quota::new(if local { DATABASE } else { limit });
                let mut reader = ScalarReader::new(&session, &database);
                let old = copy(&mut reader, ScalarRef::Utf8("old"), true, false).unwrap();
                usage(&session, &database, 3);
                let result = inputs.call(path, &mut reader);
                if limit < 4099 {
                    assert_eq!(
                        result.unwrap_err(),
                        error(Code::Limit, inputs.location(path))
                    );
                    assert_eq!(reader.allocation_count(), 0);
                    usage(&session, &database, 3);
                } else {
                    let value = result.unwrap();
                    usage(&session, &database, 4099);
                    drop(value);
                    usage(&session, &database, 3);
                }
                assert_eq!(old.as_ref(), ScalarRef::Utf8("old"));
                drop(old);
                usage(&session, &database, 0);
            }
        }
    }
}

#[test]
fn final_control_prevents_scalar_publication() {
    for raw in [
        "TRUE", "FALSE", "NULL", "?", "0", "''", "X''", "'done'", "X'00ff'",
    ] {
        let (statement, index) = fixture(raw);
        let span = statement.nodes()[index].span();
        // Allocating literals reach end once in validation, then again when charged.
        let required_charge = match raw {
            "'done'" => 4,
            "X'00ff'" => 2,
            _ => 0,
        };
        for deadline in [false, true] {
            let session = Quota::new(SESSION);
            let database = Quota::new(DATABASE);
            let mut reader = ScalarReader::new(&session, &database);
            let cancel = Cancellation::default();
            let clock = TestClock::default();
            let witnessed = Cell::new(false);
            let observer = |module, at: Span| {
                if module == Module::Value
                    && at == point(span.end)
                    && database.used() == required_charge
                {
                    witnessed.set(true);
                    if deadline {
                        clock.0.set(9);
                    } else {
                        cancel.cancel();
                    }
                }
            };
            let control =
                CallControl::new(&cancel, &clock, deadline.then_some(9)).with_observer(&observer);
            let result = reader.decode(&statement, index, &control);
            assert!(witnessed.get());
            assert_eq!(
                result.unwrap_err(),
                error(
                    if deadline {
                        Code::Timeout
                    } else {
                        Code::Cancelled
                    },
                    point(span.end)
                )
            );
            usage(&session, &database, 0);
        }
    }
}
