//! New typed predicate ownership/control inputs, frozen before implementation.
#![cfg(feature = "test-support")]

use foundation::runtime::{Cancellation, Clock, Quota};
use sql::frontend::{
    CallControl, Code, ColumnSpec, CompiledColumn, CompiledNode, CompiledPredicate, DeclaredType,
    Diagnostic, Module, Name, ParameterMetadata, ParsedStatement, Parser, PredicateCompiler,
    ScalarRef, ScalarSpec, ScalarType, Span, Token, TokenKind, TruthValue, normalize_name,
};
use std::{cell::Cell, mem::size_of};

const SESSION: u64 = 8 << 20;
const DATABASE: u64 = 64 << 20;
const SQL: &[u8] = b"a = 'text' AND ? = X'ABCD'";

#[derive(Default)]
struct TestClock(Cell<u64>);
impl Clock for TestClock {
    fn now_nanos(&self) -> u64 {
        self.0.get()
    }
}
fn point(start: usize) -> Span {
    Span { start, end: start }
}
fn error(code: Code, module: Module, span: Span) -> Diagnostic {
    Diagnostic { code, module, span }
}
fn name(raw: &str) -> Name {
    normalize_name(
        raw.as_bytes(),
        Token {
            kind: TokenKind::Identifier,
            span: Span {
                start: 0,
                end: raw.len(),
            },
        },
        &CallControl::new(&Cancellation::default(), &TestClock::default(), None),
    )
    .unwrap()
}
fn spec(declared_type: DeclaredType) -> ScalarSpec {
    ScalarSpec {
        declared_type,
        nullable: true,
    }
}
fn parse_on(source: &[u8], session: &Quota, database: &Quota) -> ParsedStatement {
    Parser::new(session, database)
        .parse_predicate(
            source,
            &CallControl::new(&Cancellation::default(), &TestClock::default(), None),
        )
        .unwrap()
}
fn parse(source: &[u8]) -> ParsedStatement {
    parse_on(source, &Quota::new(SESSION), &Quota::new(DATABASE))
}
fn compile(
    compiler: &mut PredicateCompiler,
    tree: &ParsedStatement,
    columns: &[ColumnSpec<'_>],
    hints: Option<&[Option<ScalarType>]>,
) -> Result<CompiledPredicate, Diagnostic> {
    compiler.compile(
        tree,
        columns,
        hints,
        &CallControl::new(&Cancellation::default(), &TestClock::default(), None),
    )
}
fn eval(
    output: &CompiledPredicate,
    row: &[ScalarRef<'_>],
    parameters: &[ScalarRef<'_>],
) -> Result<TruthValue, Diagnostic> {
    output.evaluate(
        row,
        parameters,
        &CallControl::new(&Cancellation::default(), &TestClock::default(), None),
    )
}
fn usage(session: &Quota, database: &Quota, n: u64) {
    assert_eq!(session.used(), n);
    assert_eq!(database.used(), n);
}
fn quotas(local: bool, bound: u64) -> (Quota, Quota) {
    (
        Quota::new(if local { bound } else { SESSION }),
        Quota::new(if local { DATABASE } else { bound }),
    )
}
fn backing_bytes(padding: usize) -> u64 {
    ((1 + padding) * size_of::<CompiledColumn>()
        + (1 + padding) * size_of::<ParameterMetadata>()
        + (9 + padding) * size_of::<CompiledNode>()
        + 6
        + 2 * padding) as u64
}
fn columns(a: &Name) -> [ColumnSpec<'_>; 1] {
    [ColumnSpec {
        name: a,
        spec: spec(DeclaredType::Utf8(65536)),
    }]
}

#[test]
fn typed_owned_lifetimes() {
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let output = {
        let a = name("a");
        let mut source = SQL.to_vec();
        let tree = parse_on(&source, &session, &database);
        source.fill(b'!');
        drop(source);
        let original = (tree.source_capacity_bytes() + tree.arena_capacity_bytes()) as u64;
        let mut compiler = PredicateCompiler::new(&session, &database);
        let hints = [Some(ScalarType::Binary)];
        let first = compile(&mut compiler, &tree, &columns(&a), Some(&hints)).unwrap();
        assert_eq!(first.nodes().len(), 9);
        assert_eq!(first.columns()[0].name_bytes(), b"A");
        assert_eq!(
            first.columns()[0].first_use(),
            Some(Span { start: 0, end: 1 })
        );
        assert_eq!(first.parameters()[0].ordinal(), 1);
        assert_eq!(
            first.parameters()[0].spec(),
            spec(DeclaredType::Binary(65536))
        );
        let second = compile(&mut compiler, &tree, &columns(&a), None).unwrap();
        usage(
            &session,
            &database,
            original + first.capacity_bytes() as u64 + second.capacity_bytes() as u64,
        );
        drop(second);
        drop(tree);
        usage(&session, &database, first.capacity_bytes() as u64);
        first
    };
    assert_eq!(
        eval(
            &output,
            &[ScalarRef::Utf8("text")],
            &[ScalarRef::Binary(&[0xab, 0xcd])]
        ),
        Ok(TruthValue::True)
    );
    assert_eq!(
        eval(
            &output,
            &[ScalarRef::Utf8("next")],
            &[ScalarRef::Binary(&[0xab, 0xcd])]
        ),
        Ok(TruthValue::False)
    );
    drop(output);
    usage(&session, &database, 0);
}

#[test]
fn typed_quota_boundaries() {
    let tree = parse(SQL);
    let a = name("a");
    assert_eq!(tree.nodes().len(), 9);
    for local in [true, false] {
        for delta in [-1_i64, 0, 1] {
            let bound = backing_bytes(0);
            let (session, database) = quotas(local, bound.checked_add_signed(delta).unwrap());
            let mut compiler = PredicateCompiler::new(&session, &database);
            let result = compile(&mut compiler, &tree, &columns(&a), None);
            if delta < 0 {
                assert_eq!(
                    result.unwrap_err(),
                    error(Code::Limit, Module::Value, point(19))
                );
                assert_eq!(session.peak(), if local { bound - 2 } else { bound });
                assert_eq!(database.peak(), bound - 2);
            } else {
                let output = result.unwrap();
                assert_eq!(output.capacity_bytes() as u64, bound);
                usage(&session, &database, bound);
                drop(output);
                assert_eq!(session.peak(), bound);
                assert_eq!(database.peak(), bound);
            }
            usage(&session, &database, 0);
        }
        let bound = backing_bytes(0);
        for delta in [-1_i64, 0, 1] {
            let (session, database) = quotas(local, (2 * bound).checked_add_signed(delta).unwrap());
            let mut compiler = PredicateCompiler::new(&session, &database);
            let old = compile(&mut compiler, &tree, &columns(&a), None).unwrap();
            let next = compile(&mut compiler, &tree, &columns(&a), None);
            if delta < 0 {
                assert_eq!(
                    next.unwrap_err(),
                    error(Code::Limit, Module::Value, point(19))
                );
                assert_eq!(
                    session.peak(),
                    if local { 2 * bound - 2 } else { 2 * bound }
                );
                assert_eq!(database.peak(), 2 * bound - 2);
            } else {
                let next = next.unwrap();
                usage(&session, &database, 2 * bound);
                drop(next);
            }
            usage(&session, &database, bound);
            assert_eq!(
                eval(
                    &old,
                    &[ScalarRef::Utf8("text")],
                    &[ScalarRef::Binary(&[0xab, 0xcd])]
                ),
                Ok(TruthValue::True)
            );
            drop(old);
            usage(&session, &database, 0);
        }
    }
}

#[test]
fn typed_actual_capacity() {
    let tree = parse(SQL);
    let a = name("a");
    for local in [true, false] {
        for delta in [-1_i64, 0, 1] {
            let bound = backing_bytes(1);
            let (session, database) = quotas(local, bound.checked_add_signed(delta).unwrap());
            let mut compiler = PredicateCompiler::new(&session, &database);
            compiler.set_allocation_padding(1);
            let result = compile(&mut compiler, &tree, &columns(&a), None);
            assert_eq!(compiler.allocation_count(), 5);
            if delta < 0 {
                assert_eq!(
                    result.unwrap_err(),
                    error(Code::Limit, Module::Value, point(19))
                );
                assert_eq!(session.peak(), if local { bound - 1 } else { bound });
                assert_eq!(database.peak(), bound - 1);
            } else {
                let output = result.unwrap();
                assert_eq!(output.capacity_bytes() as u64, bound);
                usage(&session, &database, bound);
                drop(output);
            }
            usage(&session, &database, 0);
        }
    }
    // Each typed arena's excess reservation fails after its base reservation.
    // The later arenas/payloads are never reached in these separate inputs.
    let widths = [
        size_of::<CompiledColumn>(),
        size_of::<ParameterMetadata>(),
        size_of::<CompiledNode>(),
    ];
    let lengths = [1, 1, 9];
    for local in [true, false] {
        let mut previous = 0_u64;
        for site in 0..3 {
            let base = (lengths[site] * widths[site]) as u64;
            let extra = widths[site] as u64;
            let (session, database) = quotas(local, previous + base + extra - 1);
            let mut compiler = PredicateCompiler::new(&session, &database);
            compiler.set_allocation_padding(1);
            assert_eq!(
                compile(&mut compiler, &tree, &columns(&a), None).unwrap_err(),
                error(Code::Limit, Module::Compile, point(0))
            );
            assert_eq!(compiler.allocation_count(), site + 1);
            assert_eq!(
                session.peak(),
                previous + base + if local { 0 } else { extra }
            );
            assert_eq!(database.peak(), previous + base);
            usage(&session, &database, 0);
            previous += base + extra;
        }
    }
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut compiler = PredicateCompiler::new(&session, &database);
    compiler.set_allocation_padding(usize::MAX);
    assert_eq!(
        compile(&mut compiler, &tree, &columns(&a), None).unwrap_err(),
        error(Code::Limit, Module::Compile, point(0))
    );
    assert_eq!(compiler.allocation_count(), 1);
    usage(&session, &database, 0);
    compiler.set_allocation_padding(1);
    let fixed = parse(b"1=1");
    let output = compile(&mut compiler, &fixed, &[], None).unwrap();
    assert_eq!(compiler.allocation_count(), 1);
    assert_eq!(output.capacity_bytes(), 7 * size_of::<CompiledNode>());
    drop(output);
    usage(&session, &database, 0);
    let first = compile(&mut compiler, &tree, &columns(&a), None).unwrap();
    let second = compile(&mut compiler, &tree, &columns(&a), None).unwrap();
    assert_eq!(first.capacity_bytes() as u64, backing_bytes(1));
    assert_eq!(second.capacity_bytes() as u64, backing_bytes(1));
    compiler.set_allocation_padding(0);
    let third = compile(&mut compiler, &tree, &columns(&a), None).unwrap();
    assert_eq!(third.capacity_bytes() as u64, backing_bytes(0));
    usage(&session, &database, 2 * backing_bytes(1) + backing_bytes(0));
    drop((first, second, third));
    usage(&session, &database, 0);
}

#[test]
fn typed_allocation_failures() {
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let tree = parse(SQL);
    let a = name("a");
    let mut compiler = PredicateCompiler::new(&session, &database);
    let old = compile(&mut compiler, &tree, &columns(&a), None).unwrap();
    let retained = old.capacity_bytes() as u64;
    let sites = [
        (Module::Compile, 0),
        (Module::Compile, 0),
        (Module::Compile, 0),
        (Module::Value, 4),
        (Module::Value, 19),
    ];
    for (i, (module, at)) in sites.into_iter().enumerate() {
        compiler.fail_allocation_at(i + 1);
        assert_eq!(
            compile(&mut compiler, &tree, &columns(&a), None).unwrap_err(),
            error(Code::Limit, module, point(at))
        );
        assert_eq!(compiler.allocation_count(), i + 1);
        usage(&session, &database, retained);
        drop(compile(&mut compiler, &tree, &columns(&a), None).unwrap());
        usage(&session, &database, retained);
    }
    compiler.fail_allocation_at(6);
    drop(compile(&mut compiler, &tree, &columns(&a), None).unwrap());
    assert_eq!(compiler.allocation_count(), 5);
    let six = parse(b"a='text' AND ?=X'ABCD' AND 'last'='last'");
    drop(compile(&mut compiler, &six, &columns(&a), None).unwrap());
    assert_eq!(compiler.allocation_count(), 7);
    compiler.fail_allocation_at(1);
    assert_eq!(
        compile(&mut compiler, &tree, &[], None).unwrap_err().code,
        Code::Name
    );
    assert_eq!(compiler.allocation_count(), 0);
    drop(compile(&mut compiler, &tree, &columns(&a), None).unwrap());
    let bad = parse(b"a='text' AND ?=X'Z0'");
    assert_eq!(
        compile(&mut compiler, &bad, &columns(&a), None)
            .unwrap_err()
            .code,
        Code::Syntax
    );
    usage(&session, &database, retained);
    assert_eq!(
        eval(
            &old,
            &[ScalarRef::Utf8("text")],
            &[ScalarRef::Binary(&[0xab, 0xcd])]
        ),
        Ok(TruthValue::True)
    );
    drop(old);
    usage(&session, &database, 0);
}

#[test]
fn typed_semantics_before_quota() {
    let session = Quota::new(0);
    let database = Quota::new(0);
    let mut compiler = PredicateCompiler::new(&session, &database);
    let other = Parser::new(&Quota::new(SESSION), &Quota::new(DATABASE))
        .parse(
            b"  BEGIN",
            &CallControl::new(&Cancellation::default(), &TestClock::default(), None),
        )
        .unwrap();
    assert_eq!(
        compile(&mut compiler, &other, &[], Some(&[None])).unwrap_err(),
        error(Code::Syntax, Module::Compile, Span { start: 2, end: 7 })
    );
    assert_eq!(compiler.allocation_count(), 0);
    for (source, expected) in [
        ("missing=1", Code::Name),
        ("1='x'", Code::Type),
        ("? IS NULL", Code::ParameterTypeUnknown),
    ] {
        assert_eq!(
            compile(&mut compiler, &parse(source.as_bytes()), &[], None)
                .unwrap_err()
                .code,
            expected
        );
        assert_eq!(compiler.allocation_count(), 0);
        usage(&session, &database, 0);
    }
    let a = name("a");
    let bad_metadata = [ColumnSpec {
        name: &a,
        spec: spec(DeclaredType::Utf8(0)),
    }];
    assert_eq!(
        compile(&mut compiler, &parse(b"1=1"), &bad_metadata, None).unwrap_err(),
        error(Code::Range, Module::Compile, point(0))
    );
    assert_eq!(compiler.allocation_count(), 0);
    assert_eq!(
        compile(&mut compiler, &parse(b"1=1"), &[], None).unwrap_err(),
        error(Code::Limit, Module::Compile, point(0))
    );
}

#[test]
fn typed_compile_controls() {
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let a = name("a");
    let source = format!("a='{}'", "x".repeat(2048));
    let tree = parse(source.as_bytes());
    let mut compiler = PredicateCompiler::new(&session, &database);
    let old = compile(&mut compiler, &parse(SQL), &columns(&a), None).unwrap();
    let retained = old.capacity_bytes() as u64;
    // Entry, an interior Value decode checkpoint, and final publication.
    for target in [0, 1, 2] {
        for timeout in [false, true] {
            let cancel = Cancellation::default();
            let clock = TestClock::default();
            let hit = Cell::new(false);
            let observed = Cell::new(point(0));
            let observer = |module, at: Span| {
                let matches = match target {
                    0 => module == Module::Compile && at == point(0),
                    1 => module == Module::Value && at.start > 512 && at.start < 1024,
                    2 => module == Module::Compile && at == point(source.len()),
                    _ => unreachable!(),
                };
                if matches && !hit.replace(true) {
                    observed.set(at);
                    if timeout {
                        clock.0.set(1);
                    } else {
                        cancel.cancel();
                    }
                }
            };
            let control = CallControl::new(&cancel, &clock, Some(1)).with_observer(&observer);
            let got = compiler
                .compile(&tree, &columns(&a), None, &control)
                .unwrap_err();
            assert!(hit.get());
            assert_eq!(
                got,
                error(
                    if timeout {
                        Code::Timeout
                    } else {
                        Code::Cancelled
                    },
                    if target == 1 {
                        Module::Value
                    } else {
                        Module::Compile
                    },
                    observed.get()
                )
            );
            usage(&session, &database, retained);
            drop(compile(&mut compiler, &tree, &columns(&a), None).unwrap());
            usage(&session, &database, retained);
        }
    }
    let cancel = Cancellation::default();
    cancel.cancel();
    assert_eq!(
        compiler
            .compile(
                &tree,
                &columns(&a),
                None,
                &CallControl::new(&cancel, &TestClock(Cell::new(1)), Some(1))
            )
            .unwrap_err()
            .code,
        Code::Cancelled
    );
    let metadata_names: Vec<_> = (0..32).map(|i| name(&format!("m{i}"))).collect();
    let mut metadata: Vec<_> = metadata_names
        .iter()
        .map(|n| ColumnSpec {
            name: n,
            spec: spec(DeclaredType::Int64),
        })
        .collect();
    metadata[31].spec = spec(DeclaredType::Utf8(0));
    let constant = parse(b"1=1");
    for timeout in [false, true] {
        let cancel = Cancellation::default();
        let clock = TestClock::default();
        let seen = Cell::new(0);
        let observer = |module, at| {
            if module == Module::Compile && at == point(0) {
                seen.set(seen.get() + 1);
                if seen.get() == 10 {
                    if timeout {
                        clock.0.set(1);
                    } else {
                        cancel.cancel();
                    }
                }
            }
        };
        let control = CallControl::new(&cancel, &clock, Some(1)).with_observer(&observer);
        assert_eq!(
            compiler
                .compile(&constant, &metadata, None, &control)
                .unwrap_err(),
            error(
                if timeout {
                    Code::Timeout
                } else {
                    Code::Cancelled
                },
                Module::Compile,
                point(0)
            )
        );
        assert_eq!(seen.get(), 10);
        assert_eq!(compiler.allocation_count(), 0);
        usage(&session, &database, retained);
    }
    // The RHS is visited only a bounded number of times outside the candidate
    // loop. Ten checks at its distinct source position witness interior lookup.
    let names: Vec<_> = (0..32)
        .map(|i| name(&format!("{}{:02}", "a".repeat(126), i)))
        .collect();
    let metadata: Vec<_> = names
        .iter()
        .map(|n| ColumnSpec {
            name: n,
            spec: spec(DeclaredType::Int64),
        })
        .collect();
    let lookup_source = format!("1 = {}31", "a".repeat(126));
    let lookup = parse(lookup_source.as_bytes());
    for timeout in [false, true] {
        let cancel = Cancellation::default();
        let clock = TestClock::default();
        let seen = Cell::new(0);
        let observer = |module, at| {
            if module == Module::Compile && at == point(4) {
                seen.set(seen.get() + 1);
                if seen.get() == 10 {
                    if timeout {
                        clock.0.set(1);
                    } else {
                        cancel.cancel();
                    }
                }
            }
        };
        let control = CallControl::new(&cancel, &clock, Some(1)).with_observer(&observer);
        assert_eq!(
            compiler
                .compile(&lookup, &metadata, None, &control)
                .unwrap_err(),
            error(
                if timeout {
                    Code::Timeout
                } else {
                    Code::Cancelled
                },
                Module::Compile,
                point(4)
            )
        );
        assert_eq!(seen.get(), 10);
        assert_eq!(compiler.allocation_count(), 0);
        usage(&session, &database, retained);
    }
    drop(old);
    usage(&session, &database, 0);
}

#[test]
fn typed_eval_controls() {
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut compiler = PredicateCompiler::new(&session, &database);
    let source = b"1=1 AND 2=2";
    let output = compile(&mut compiler, &parse(source), &[], None).unwrap();
    for at in [point(0), point(8), point(source.len())] {
        for timeout in [false, true] {
            let cancel = Cancellation::default();
            let clock = TestClock::default();
            let hit = Cell::new(false);
            let observer = |module, span| {
                if module == Module::Eval && span == at && !hit.replace(true) {
                    if timeout {
                        clock.0.set(1);
                    } else {
                        cancel.cancel();
                    }
                }
            };
            let control = CallControl::new(&cancel, &clock, Some(1)).with_observer(&observer);
            assert_eq!(
                output.evaluate(&[], &[], &control),
                Err(error(
                    if timeout {
                        Code::Timeout
                    } else {
                        Code::Cancelled
                    },
                    Module::Eval,
                    at
                ))
            );
            assert!(hit.get());
            assert_eq!(eval(&output, &[], &[]), Ok(TruthValue::True));
        }
    }
    let cancel = Cancellation::default();
    let clock = TestClock::default();
    assert_eq!(
        output.evaluate(&[], &[], &CallControl::new(&cancel, &clock, None)),
        Ok(TruthValue::True)
    );
    cancel.cancel();
    assert_eq!(eval(&output, &[], &[]), Ok(TruthValue::True));
    drop(output);
    usage(&session, &database, 0);
}

#[test]
fn typed_long_compare_controls() {
    let a = name("a");
    let tree = parse(format!("{}a=?", " ".repeat(65537)).as_bytes());
    let text = "x".repeat(65536);
    let bytes = vec![0x80; 65536];
    for binary in [false, true] {
        let session = Quota::new(SESSION);
        let database = Quota::new(DATABASE);
        let mut compiler = PredicateCompiler::new(&session, &database);
        let columns = [ColumnSpec {
            name: &a,
            spec: spec(if binary {
                DeclaredType::Binary(65536)
            } else {
                DeclaredType::Utf8(65536)
            }),
        }];
        let output = compile(&mut compiler, &tree, &columns, None).unwrap();
        let value = if binary {
            ScalarRef::Binary(&bytes)
        } else {
            ScalarRef::Utf8(&text)
        };
        let count = Cell::new(0);
        let last = Cell::new(0);
        let observer = |module, at: Span| {
            if module == Module::Eval && at.start <= 65536 {
                assert!(at.start >= last.get());
                assert!(at.start - last.get() <= 128);
                last.set(at.start);
                count.set(count.get() + 1);
            }
        };
        let cancel = Cancellation::default();
        let clock = TestClock::default();
        assert_eq!(
            output.evaluate(
                &[value],
                &[value],
                &CallControl::new(&cancel, &clock, None).with_observer(&observer)
            ),
            Ok(TruthValue::True)
        );
        assert_eq!(last.get(), 65536);
        assert!(count.get() >= 512);
        for timeout in [false, true] {
            let cancel = Cancellation::default();
            let clock = TestClock::default();
            let hit = Cell::new(false);
            let observer = |module, at: Span| {
                if module == Module::Eval && (32768..32896).contains(&at.start) {
                    hit.set(true);
                    if timeout {
                        clock.0.set(1);
                    } else {
                        cancel.cancel();
                    }
                }
            };
            let control = CallControl::new(&cancel, &clock, Some(1)).with_observer(&observer);
            assert_eq!(
                output.evaluate(&[value], &[value], &control),
                Err(error(
                    if timeout {
                        Code::Timeout
                    } else {
                        Code::Cancelled
                    },
                    Module::Eval,
                    Span {
                        start: 65537,
                        end: 65540
                    }
                ))
            );
            assert!(hit.get());
            assert_eq!(eval(&output, &[value], &[value]), Ok(TruthValue::True));
        }
        let eager_tree = parse(b"0=1 AND a=?");
        let eager = compile(&mut compiler, &eager_tree, &columns, None).unwrap();
        let last = Cell::new(0);
        let observer = |module, at: Span| {
            if module == Module::Eval && at.start >= 128 {
                last.set(at.start);
            }
        };
        assert_eq!(
            eager.evaluate(
                &[value],
                &[value],
                &CallControl::new(&Cancellation::default(), &TestClock::default(), None)
                    .with_observer(&observer)
            ),
            Ok(TruthValue::False)
        );
        assert_eq!(last.get(), 65536);
        drop(eager);
        drop(output);
        usage(&session, &database, 0);
    }
    // Source positions are all >=20, making local comparison progress 0/1
    // distinguishable from node/input traversal and final publication.
    let short = parse(b"                    a=?");
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut compiler = PredicateCompiler::new(&session, &database);
    let columns = [ColumnSpec {
        name: &a,
        spec: spec(DeclaredType::Binary(65536)),
    }];
    let output = compile(&mut compiler, &short, &columns, None).unwrap();
    for empty in [false, true] {
        for timeout in [false, true] {
            let cancel = Cancellation::default();
            let clock = TestClock::default();
            let zeros = Cell::new(0);
            let hit = Cell::new(false);
            let observer = |module, at| {
                if module != Module::Eval {
                    return;
                }
                if at == point(0) {
                    zeros.set(zeros.get() + 1);
                }
                // Empty: entry, comparison entry, then comparison final.
                if (empty && at == point(0) && zeros.get() == 3) || (!empty && at == point(1)) {
                    hit.set(true);
                    if timeout {
                        clock.0.set(1);
                    } else {
                        cancel.cancel();
                    }
                }
            };
            let control = CallControl::new(&cancel, &clock, Some(1)).with_observer(&observer);
            let (left, right): (&[u8], &[u8]) = if empty {
                (&[], &[])
            } else {
                (&[0x10], &[0x20])
            };
            assert_eq!(
                output.evaluate(
                    &[ScalarRef::Binary(left)],
                    &[ScalarRef::Binary(right)],
                    &control
                ),
                Err(error(
                    if timeout {
                        Code::Timeout
                    } else {
                        Code::Cancelled
                    },
                    Module::Eval,
                    Span { start: 20, end: 23 }
                ))
            );
            assert!(hit.get());
        }
    }
    drop(output);
    usage(&session, &database, 0);
}

#[test]
fn typed_validation_controls() {
    for parameters in [false, true] {
        let count = if parameters { 256 } else { 32 };
        let names: Vec<_> = (0..count).map(|i| name(&format!("v{i}"))).collect();
        let metadata: Vec<_> = names
            .iter()
            .map(|n| ColumnSpec {
                name: n,
                spec: spec(DeclaredType::Utf8(65536)),
            })
            .collect();
        let mut offsets = Vec::new();
        let mut source = String::new();
        for i in 0..count {
            if i != 0 {
                source.push_str(" OR ");
            }
            offsets.push(source.len());
            source.push_str(&if parameters {
                "? IS NULL".to_owned()
            } else {
                format!("v{i} IS NULL")
            });
        }
        let hints = vec![Some(ScalarType::Utf8); count];
        let session = Quota::new(SESSION);
        let database = Quota::new(DATABASE);
        let mut compiler = PredicateCompiler::new(&session, &database);
        let output = compile(
            &mut compiler,
            &parse(source.as_bytes()),
            if parameters { &[] } else { &metadata },
            if parameters { Some(&hints) } else { None },
        )
        .unwrap();
        let large = "x".repeat(65537);
        for length_pass in [false, true] {
            let mut inputs = vec![ScalarRef::Utf8("x"); count];
            inputs[count - 1] = if length_pass {
                ScalarRef::Utf8(&large)
            } else {
                ScalarRef::Null(ScalarType::Bool)
            };
            for timeout in [false, true] {
                let cancel = Cancellation::default();
                let clock = TestClock::default();
                let armed = Cell::new(!length_pass);
                let hit = Cell::new(false);
                let observer = |module, at| {
                    if module != Module::Eval {
                        return;
                    }
                    // Seeing the last input's tag check arms the later length
                    // pass, so a tag-loop check cannot satisfy its detector.
                    if at == point(offsets[count - 1]) {
                        armed.set(true);
                    }
                    if armed.get() && at == point(offsets[count / 2]) {
                        hit.set(true);
                        if timeout {
                            clock.0.set(1);
                        } else {
                            cancel.cancel();
                        }
                    }
                };
                let control = CallControl::new(&cancel, &clock, Some(1)).with_observer(&observer);
                let result = if parameters {
                    output.evaluate(&[], &inputs, &control)
                } else {
                    output.evaluate(&inputs, &[], &control)
                };
                assert_eq!(
                    result,
                    Err(error(
                        if timeout {
                            Code::Timeout
                        } else {
                            Code::Cancelled
                        },
                        Module::Eval,
                        point(offsets[count / 2])
                    ))
                );
                assert!(hit.get());
            }
        }
        drop(output);
        usage(&session, &database, 0);
    }
}

#[test]
fn typed_eval_no_new_charge() {
    let a = name("a");
    let tree = parse(b"a=?");
    let bound = (size_of::<CompiledColumn>()
        + size_of::<ParameterMetadata>()
        + 6 * size_of::<CompiledNode>()) as u64;
    let session = Quota::new(bound);
    let database = Quota::new(bound);
    let mut compiler = PredicateCompiler::new(&session, &database);
    let output = compile(&mut compiler, &tree, &columns(&a), None).unwrap();
    assert_eq!(output.capacity_bytes() as u64, bound);
    let value = "x".repeat(65536);
    for parameter in [value.as_str(), "other", ""] {
        let expected = match parameter.len() {
            65536 => TruthValue::True,
            0 => TruthValue::Unknown,
            _ => TruthValue::False,
        };
        assert_eq!(
            eval(
                &output,
                &[ScalarRef::Utf8(&value)],
                &[ScalarRef::Utf8(parameter)]
            ),
            Ok(expected)
        );
        usage(&session, &database, bound);
        assert_eq!(session.peak(), bound);
        assert_eq!(database.peak(), bound);
    }
    assert_eq!(
        eval(&output, &[ScalarRef::Int64(0)], &[ScalarRef::Utf8("")])
            .unwrap_err()
            .code,
        Code::Type
    );
    usage(&session, &database, bound);
    drop(output);
    usage(&session, &database, 0);
}

#[test]
fn typed_stack_and_drop() {
    assert!(size_of::<PredicateCompiler>() <= 256);
    assert!(size_of::<CompiledPredicate>() <= 512);
    assert!(size_of::<CompiledNode>() <= 256);
    assert!(size_of::<CompiledColumn>() <= 256);
    assert!(size_of::<ParameterMetadata>() <= 64);
    assert_eq!(size_of::<[TruthValue; 8192]>(), 8192);
    let nested = parse(format!("{}1=1{}", "(".repeat(64), ")".repeat(64)).as_bytes());
    // 3 wrapper + 2729 * 3 operand/comparison + 2 NOT = 8192.
    let flat = parse(format!("NOT NOT {}", vec!["1=1"; 2729].join(" AND ")).as_bytes());
    assert_eq!(flat.nodes().len(), 8192);
    let markers = parse(vec!["?=1"; 256].join(" AND ").as_bytes());
    assert_eq!(markers.marker_count(), 256);
    let names: Vec<_> = (0..32).map(|i| name(&format!("c{i}"))).collect();
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let local = session.clone();
    let shared = database.clone();
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || {
            let columns: Vec<_> = names
                .iter()
                .map(|n| ColumnSpec {
                    name: n,
                    spec: spec(DeclaredType::Int64),
                })
                .collect();
            let mut compiler = PredicateCompiler::new(&local, &shared);
            for tree in [nested, flat, markers] {
                let output = compile(&mut compiler, &tree, &columns, None).unwrap();
                let params = [ScalarRef::Int64(1); 256];
                assert_eq!(
                    eval(
                        &output,
                        &[ScalarRef::Int64(1); 32],
                        &params[..usize::from(tree.marker_count())]
                    ),
                    Ok(TruthValue::True)
                );
                drop(output);
                drop(tree);
                usage(&local, &shared, 0);
            }
        })
        .unwrap()
        .join()
        .unwrap();
    usage(&session, &database, 0);
}

#[test]
fn typed_shared_database() {
    let tree = parse(SQL);
    let bytes = backing_bytes(0);
    for delta in [-1_i64, 0, 1] {
        let database = Quota::new((2 * bytes).checked_add_signed(delta).unwrap());
        let first_session = Quota::new(SESSION);
        let second_session = Quota::new(SESSION);
        std::thread::scope(|scope| {
            // Keep the first native caller's result in the parent while the
            // second native caller allocates. No worker can strand a barrier
            // when a product panic or a failed assertion is detected.
            let first = scope
                .spawn(|| {
                    let a = name("a");
                    let mut compiler = PredicateCompiler::new(&first_session, &database);
                    compile(&mut compiler, &tree, &columns(&a), None)
                })
                .join()
                .unwrap()
                .unwrap();
            let second = scope
                .spawn(|| {
                    let a = name("a");
                    let mut compiler = PredicateCompiler::new(&second_session, &database);
                    compile(&mut compiler, &tree, &columns(&a), None)
                })
                .join()
                .unwrap();
            if delta < 0 {
                assert_eq!(
                    second.unwrap_err(),
                    error(Code::Limit, Module::Value, point(19))
                );
                assert_eq!(database.used(), bytes);
            } else {
                let second = second.unwrap();
                assert_eq!(database.used(), 2 * bytes);
                drop(second);
            }
            assert_eq!(second_session.used(), 0);
            assert_eq!(
                eval(
                    &first,
                    &[ScalarRef::Utf8("text")],
                    &[ScalarRef::Binary(&[0xab, 0xcd])]
                ),
                Ok(TruthValue::True)
            );
            drop(first);
        });
        assert_eq!(first_session.used(), 0);
        usage(&second_session, &database, 0);
    }
}
