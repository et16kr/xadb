//! P4 resource inputs proposed before product implementation and D2 acceptance.
//! Every runtime route starts a newly supported statement; no old suite is run.
#![cfg(feature = "test-support")]

use foundation::runtime::{Cancellation, Clock, Quota};
use sql::frontend::{
    AstNode, CallControl, Code, Diagnostic, Module, NodeKind, NullabilitySyntax, ParsedStatement,
    Parser, ScalarSyntax, Sign, Span, TypeSyntax,
};
use std::{cell::Cell, mem::size_of};

const SESSION: u64 = 8 << 20;
const DATABASE: u64 = 64 << 20;
const SMALL: &[u8] = b"SELECT * FROM t";
const ROUTES: &[(&[u8], usize)] = &[
    (SMALL, 5),
    (b"UPDATE t SET a=0", 7),
    (b"DELETE FROM t", 3),
    (b"CREATE TABLE t(a BIGINT) STORAGE DISK", 8),
    (b"CREATE INDEX i ON t(a)", 7),
    (b"CREATE UNIQUE INDEX i ON t(a)", 7),
    (b"DROP TABLE t", 3),
    (b"DROP INDEX i", 3),
];

#[derive(Default)]
struct TestClock(Cell<u64>);
impl Clock for TestClock {
    fn now_nanos(&self) -> u64 {
        self.0.get()
    }
}

// Validate a focused witness before any product call. Normal selection is None.
fn resource_case(allowed: impl FnOnce(&str) -> bool) -> Option<String> {
    for name in ["XADB_REMAINING_CASE", "XADB_REMAINING_FROM_CASE"] {
        assert!(
            std::env::var_os(name).is_none(),
            "{name} is not a resource witness selector"
        );
    }
    let value = match std::env::var("XADB_REMAINING_RESOURCE_CASE") {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => return None,
        Err(std::env::VarError::NotUnicode(_)) => {
            panic!("XADB_REMAINING_RESOURCE_CASE must be Unicode")
        }
    };
    assert!(allowed(&value), "unknown resource witness: {value}");
    Some(value)
}

fn selected_case(selected: &Option<String>, id: &str) -> bool {
    selected.is_none() || selected.as_deref() == Some(id)
}

fn point(start: usize) -> Span {
    Span { start, end: start }
}

fn error(code: Code, module: Module, span: Span) -> Diagnostic {
    Diagnostic { code, module, span }
}

fn limit(at: usize) -> Diagnostic {
    error(Code::Limit, Module::Parse, point(at))
}

fn parse(parser: &mut Parser, source: &[u8]) -> Result<ParsedStatement, Diagnostic> {
    parser.parse(
        source,
        &CallControl::new(&Cancellation::default(), &TestClock::default(), None),
    )
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

fn quotas(local: bool, bound: u64) -> (Quota, Quota) {
    (
        Quota::new(if local { bound } else { SESSION }),
        Quota::new(if local { DATABASE } else { bound }),
    )
}

fn locate(source: &[u8], literal: &[u8]) -> usize {
    source
        .windows(literal.len())
        .position(|w| w == literal)
        .unwrap()
}

fn update(assignments: usize) -> Vec<u8> {
    let mut source = b"UPDATE t SET ".to_vec();
    for index in 0..assignments {
        if index != 0 {
            source.push(b',');
        }
        source.extend_from_slice(b"a=0");
    }
    source
}

fn table(columns: usize, qualified: bool) -> Vec<u8> {
    let mut source = if qualified {
        b"CREATE TABLE PUBLIC.t(".to_vec()
    } else {
        b"CREATE TABLE t(".to_vec()
    };
    for index in 0..columns {
        if index != 0 {
            source.push(b',');
        }
        source.extend_from_slice(b"a BIGINT");
    }
    source.extend_from_slice(b") STORAGE DISK");
    source
}

fn nested(prefix: &str, depth: usize) -> Vec<u8> {
    format!("{prefix}{}0=0{}", "(".repeat(depth), ")".repeat(depth)).into_bytes()
}

fn control_failure(
    parser: &mut Parser,
    source: &[u8],
    database: &Quota,
    at: usize,
    timeout: bool,
) -> Diagnostic {
    let token = Cancellation::default();
    let clock = TestClock::default();
    let seen = Cell::new(false);
    let observer = |module, span: Span| {
        // Excludes UTF-8/lexical checks and source copying before the arena exists.
        if module == Module::Parse && span == point(at) && database.used() > source.len() as u64 {
            seen.set(true);
            if timeout {
                clock.0.set(10);
            } else {
                token.cancel();
            }
        }
    };
    let result = parser.parse(
        source,
        &CallControl::new(&token, &clock, Some(10)).with_observer(&observer),
    );
    assert!(seen.get(), "missing Parse checkpoint {at}: {source:?}");
    result.unwrap_err()
}

#[test]
fn remaining_owned_lifetimes() {
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut parser = Parser::new(&session, &database);
    let original = b"CREATE TABLE public.t(a VARCHAR(-0007) NULL,b BIGINT NOT NULL,PRIMARY KEY(b),UNIQUE(a)) STORAGE DISK;";
    let mut caller = original.to_vec();
    let first = parse(&mut parser, &caller).unwrap();
    caller.fill(b'!');
    drop(caller);
    assert_eq!(first.source(), original);
    assert_eq!(first.nodes().len(), 18);
    assert_eq!(first.marker_count(), 0);
    assert_eq!(first.nodes()[0].span().end, original.len() - 1);
    let lengths = first
        .nodes()
        .iter()
        .filter_map(|node| match node.kind() {
            NodeKind::DataType(TypeSyntax::Utf8(length)) => Some(*length),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(lengths.len(), 1);
    assert_eq!(lengths[0].sign, Sign::Minus);
    assert_eq!(
        &first.source()[lengths[0].digits.start..lengths[0].digits.end],
        b"0007"
    );
    assert_eq!(
        &first.source()[lengths[0].span.start..lengths[0].span.end],
        b"-0007"
    );
    let nullability = first
        .nodes()
        .iter()
        .filter_map(|node| match node.kind() {
            NodeKind::ColumnDefinition { nullability } => Some(*nullability),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        nullability,
        [NullabilitySyntax::Null, NullabilitySyntax::NotNull]
    );
    let held = charge(&first);
    let second = parse(&mut parser, b"UPDATE t SET a=?,b=1 WHERE a=? AND b=?").unwrap();
    assert_eq!(second.nodes().len(), 19);
    assert_eq!(second.marker_count(), 3);
    let ordinals = second
        .nodes()
        .iter()
        .filter_map(|node| match node.kind() {
            NodeKind::Scalar(ScalarSyntax::Parameter { ordinal }) => Some(*ordinal),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(ordinals, [1, 2, 3]);
    let where_node = second
        .nodes()
        .iter()
        .find(|node| matches!(node.kind(), NodeKind::Where))
        .unwrap();
    assert!(matches!(
        second.nodes()[where_node.first_child().unwrap()].kind(),
        NodeKind::Or
    ));
    assert!(
        !second
            .nodes()
            .iter()
            .any(|node| matches!(node.kind(), NodeKind::Predicate))
    );
    let second_charge = charge(&second);
    usage(&session, &database, held + second_charge);
    drop(parser);
    drop(first);
    assert_eq!(second.source(), b"UPDATE t SET a=?,b=1 WHERE a=? AND b=?");
    usage(&session, &database, second_charge);
    drop(second);
    usage(&session, &database, 0);
}

#[test]
fn remaining_quota_boundaries() {
    for &(source, count) in ROUTES {
        for local in [true, false] {
            for delta in [-1_i64, 0, 1] {
                let bound = source.len() as u64 + 16 * node_bytes();
                let (session, database) = quotas(local, bound.checked_add_signed(delta).unwrap());
                let mut parser = Parser::new(&session, &database);
                let result = parse(&mut parser, source);
                if delta < 0 {
                    assert_eq!(result.unwrap_err(), limit(0));
                    assert_eq!(parser.allocation_count(), 1);
                    assert_eq!(
                        session.peak(),
                        if local { source.len() as u64 } else { bound }
                    );
                    assert_eq!(database.peak(), source.len() as u64);
                } else {
                    let tree = result.unwrap();
                    assert_eq!(tree.nodes().len(), count);
                    assert_eq!(charge(&tree), bound);
                    usage(&session, &database, bound);
                    drop(tree);
                }
                usage(&session, &database, 0);
            }
        }
    }
    let source = update(5);
    assert_eq!(source.len(), 32);
    for local in [true, false] {
        for delta in [-1_i64, 0, 1] {
            let held = 15 + 16 * node_bytes();
            let peak = held + 32 + 48 * node_bytes();
            let (session, database) = quotas(local, peak.checked_add_signed(delta).unwrap());
            let mut parser = Parser::new(&session, &database);
            let old = parse(&mut parser, SMALL).unwrap();
            let result = parse(&mut parser, &source);
            if delta < 0 {
                assert_eq!(result.unwrap_err(), limit(29));
                let before = held + 32 + 16 * node_bytes();
                assert_eq!(session.peak(), if local { before } else { peak });
                assert_eq!(database.peak(), before);
            } else {
                let tree = result.unwrap();
                assert_eq!(tree.nodes().len(), 19);
                assert_eq!(charge(&tree), 32 + 32 * node_bytes());
                usage(&session, &database, held + charge(&tree));
                assert_eq!(session.peak(), peak);
                assert_eq!(database.peak(), peak);
                drop(tree);
            }
            assert_eq!(old.source(), SMALL);
            usage(&session, &database, held);
            drop(old);
            usage(&session, &database, 0);
        }
    }
}

#[test]
fn remaining_actual_capacity() {
    for local in [true, false] {
        for delta in [-1_i64, 0, 1] {
            let peak = 33 + 52 * node_bytes();
            let (session, database) = quotas(local, peak.checked_add_signed(delta).unwrap());
            let mut parser = Parser::new(&session, &database);
            parser.set_allocation_padding(1);
            let result = parse(&mut parser, &update(5));
            if delta < 0 {
                assert_eq!(result.unwrap_err(), limit(29));
                let before_extra = 33 + 51 * node_bytes();
                assert_eq!(session.peak(), if local { before_extra } else { peak });
                assert_eq!(database.peak(), before_extra);
            } else {
                let tree = result.unwrap();
                assert_eq!(tree.source_capacity_bytes(), 33);
                assert_eq!(tree.arena_capacity_bytes() as u64, 35 * node_bytes());
                assert_eq!(charge(&tree), 33 + 35 * node_bytes());
                usage(&session, &database, charge(&tree));
                assert_eq!(database.peak(), peak);
                drop(tree);
            }
            assert_eq!(parser.allocation_count(), 3);
            usage(&session, &database, 0);
        }
        for (bound, count) in [(15, 1), (16 + 16 * node_bytes(), 2)] {
            let (session, database) = quotas(local, bound);
            let mut parser = Parser::new(&session, &database);
            parser.set_allocation_padding(1);
            assert_eq!(parse(&mut parser, SMALL).unwrap_err(), limit(0));
            assert_eq!(parser.allocation_count(), count);
            usage(&session, &database, 0);
        }
    }
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut parser = Parser::new(&session, &database);
    parser.set_allocation_padding(1);
    let a = parse(&mut parser, SMALL).unwrap();
    let b = parse(&mut parser, SMALL).unwrap();
    assert_eq!(charge(&a), 16 + 17 * node_bytes());
    assert_eq!(charge(&a), charge(&b));
    let held = charge(&a) + charge(&b);
    parser.set_allocation_padding(usize::MAX);
    assert_eq!(parse(&mut parser, SMALL).unwrap_err(), limit(0));
    assert_eq!(parser.allocation_count(), 1);
    usage(&session, &database, held);
    parser.set_allocation_padding(0);
    let c = parse(&mut parser, SMALL).unwrap();
    assert_eq!(charge(&c), 15 + 16 * node_bytes());
    drop((a, b, c));
    usage(&session, &database, 0);
}

#[test]
fn remaining_allocation_failures() {
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut parser = Parser::new(&session, &database);
    let old = parse(&mut parser, SMALL).unwrap();
    let held = charge(&old);
    // Independent preorder arithmetic: prefix has 4 nodes, each a=0 has 3.
    // Source offset is 13+4*i (assignment/name), or 15+4*i (scalar).
    let large = update(2700);
    for (index, at) in [0, 0, 29, 49, 93, 177, 349, 689, 1373, 2737, 5469]
        .into_iter()
        .enumerate()
    {
        let ordinal = index + 1;
        parser.fail_allocation_at(ordinal);
        assert_eq!(parse(&mut parser, &large).unwrap_err(), limit(at));
        assert_eq!(parser.allocation_count(), ordinal);
        usage(&session, &database, held);
        assert_eq!(old.source(), SMALL);
        drop(parse(&mut parser, b"DROP INDEX i").unwrap());
        usage(&session, &database, held);
    }
    for &(source, _) in ROUTES {
        for ordinal in [1, 2] {
            parser.fail_allocation_at(ordinal);
            assert_eq!(parse(&mut parser, source).unwrap_err(), limit(0));
            assert_eq!(parser.allocation_count(), ordinal);
            usage(&session, &database, held);
        }
    }
    parser.fail_allocation_at(4);
    let small = parse(&mut parser, &update(5)).unwrap();
    assert_eq!(parser.allocation_count(), 3);
    drop(small);
    let large = parse(&mut parser, &large).unwrap();
    assert_eq!(large.nodes().len(), 8104);
    assert_eq!(parser.allocation_count(), 11);
    drop(large);
    let mut malformed = table(300, false);
    malformed.extend_from_slice(b" extra");
    assert_eq!(
        parse(&mut parser, &malformed).unwrap_err().code,
        Code::Syntax
    );
    usage(&session, &database, held);
    drop(old);
    usage(&session, &database, 0);
}

#[test]
fn remaining_admission_order() {
    let selected = resource_case(|id| {
        matches!(
            id,
            "admission/roots"
                | "admission/where"
                | "admission/variable-datatype"
                | "admission/unit-datatype"
        )
    });
    if selected_case(&selected, "admission/roots") {
        for source in [b"CREATE @".as_slice(), b"DROP @", b"SELECT @"] {
            for local in [true, false] {
                let (session, database) =
                    quotas(local, source.len() as u64 + 16 * node_bytes() - 1);
                let mut parser = Parser::new(&session, &database);
                assert_eq!(parse(&mut parser, source).unwrap_err(), limit(0));
                assert_eq!(parser.allocation_count(), 1);
                usage(&session, &database, 0);
            }
        }
    }
    if selected_case(&selected, "admission/where") {
        // 14 nodes precede WHERE, then Where(15), Or(16), And needs growth at EOF.
        let source = b"SELECT a,b,c,d,e FROM t WHERE";
        for local in [true, false] {
            let (session, database) = quotas(local, source.len() as u64 + 16 * node_bytes());
            let mut parser = Parser::new(&session, &database);
            assert_eq!(parse(&mut parser, source).unwrap_err(), limit(source.len()));
            assert_eq!(parser.allocation_count(), 2);
            usage(&session, &database, 0);
        }
        let session = Quota::new(SESSION);
        let database = Quota::new(DATABASE);
        let mut parser = Parser::new(&session, &database);
        assert_eq!(
            parse(&mut parser, source).unwrap_err(),
            error(Code::Syntax, Module::Parse, point(source.len()))
        );
        usage(&session, &database, 0);
    }
    if selected_case(&selected, "admission/variable-datatype") {
        // Four column names leave exactly16 nodes. A variable DataType is appended
        // only after its complete raw format, so '?' must win over arena growth.
        let malformed =
            b"CREATE TABLE PUBLIC.t(a BIGINT,b BIGINT,c BIGINT,d VARCHAR(?)) STORAGE DISK";
        let marker = locate(malformed, b"?");
        for local in [true, false] {
            let (session, database) = quotas(local, malformed.len() as u64 + 16 * node_bytes());
            let mut parser = Parser::new(&session, &database);
            assert_eq!(
                parse(&mut parser, malformed).unwrap_err(),
                error(
                    Code::Syntax,
                    Module::Parse,
                    Span {
                        start: marker,
                        end: marker + 1
                    }
                )
            );
            assert_eq!(parser.allocation_count(), 2);
            usage(&session, &database, 0);
        }
    }
    if selected_case(&selected, "admission/unit-datatype") {
        // BIGINT is complete at its keyword; its17th node admission may precede a
        // later malformed '(' suffix. This is a distinct, deliberate ordering.
        let complete_type =
            b"CREATE TABLE PUBLIC.t(a BIGINT,b BIGINT,c BIGINT,d BIGINT(1)) STORAGE DISK";
        let type_at = locate(complete_type, b"d BIGINT") + 2;
        for local in [true, false] {
            let (session, database) = quotas(local, complete_type.len() as u64 + 16 * node_bytes());
            let mut parser = Parser::new(&session, &database);
            assert_eq!(
                parse(&mut parser, complete_type).unwrap_err(),
                limit(type_at)
            );
            assert_eq!(parser.allocation_count(), 2);
            usage(&session, &database, 0);
        }
    }
}

#[test]
fn remaining_entry_final_controls() {
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut parser = Parser::new(&session, &database);
    let token = Cancellation::default();
    token.cancel();
    let expired = TestClock(Cell::new(10));
    parser.fail_allocation_at(1);
    assert_eq!(
        parser
            .parse(SMALL, &CallControl::new(&token, &expired, Some(10)))
            .unwrap_err(),
        error(Code::Cancelled, Module::Parse, point(0))
    );
    assert_eq!(parser.allocation_count(), 0);
    drop(parse(&mut parser, SMALL).unwrap());
    assert_eq!(
        parser
            .parse(
                SMALL,
                &CallControl::new(&Cancellation::default(), &expired, Some(10))
            )
            .unwrap_err(),
        error(Code::Timeout, Module::Parse, point(0))
    );
    for &(source, _) in ROUTES {
        let mut trailing = source.to_vec();
        trailing.extend_from_slice(b"; /* final gap */  ");
        for timeout in [false, true] {
            assert_eq!(
                control_failure(&mut parser, &trailing, &database, trailing.len(), timeout),
                error(
                    if timeout {
                        Code::Timeout
                    } else {
                        Code::Cancelled
                    },
                    Module::Parse,
                    point(trailing.len())
                )
            );
            usage(&session, &database, 0);
            drop(parse(&mut parser, source).unwrap());
        }
    }
    usage(&session, &database, 0);
}

#[test]
fn remaining_classification_controls() {
    let mut full = b"SELECT ".to_vec();
    for index in 0..32 {
        if index != 0 {
            full.push(b',');
        }
        full.push(b'a');
    }
    full.extend_from_slice(b",   ");
    let at = full.len();
    let mut name = full.clone();
    name.extend_from_slice(b"z FROM t");
    let mut expression = full;
    expression.extend_from_slice(b"1 FROM t");
    let constraint = b"CREATE TABLE t(a BIGINT,   CHECK(a)) STORAGE DISK".to_vec();
    let constraint_at = locate(&constraint, b"CHECK");
    for (source, at, pending) in [
        (name, at, Code::Limit),
        (expression, at, Code::Unsupported),
        (constraint, constraint_at, Code::Unsupported),
    ] {
        for timeout in [false, true] {
            let session = Quota::new(SESSION);
            let database = Quota::new(DATABASE);
            let mut parser = Parser::new(&session, &database);
            assert_eq!(
                control_failure(&mut parser, &source, &database, at, timeout),
                error(
                    if timeout {
                        Code::Timeout
                    } else {
                        Code::Cancelled
                    },
                    Module::Parse,
                    point(at)
                )
            );
            usage(&session, &database, 0);
            assert_eq!(parse(&mut parser, &source).unwrap_err().code, pending);
            usage(&session, &database, 0);
        }
    }
}

#[test]
fn remaining_list_completion_controls() {
    let cases: &[(&str, &[u8], &[u8], usize)] = &[
        (
            "completion/projection-comma",
            b"SELECT a,   b FROM t",
            b",",
            1,
        ),
        (
            "completion/projection-item",
            b"SELECT a /* gap */ FROM t",
            b"a",
            1,
        ),
        (
            "completion/assignment-comma",
            b"UPDATE t SET a=0,   b=1",
            b",",
            1,
        ),
        (
            "completion/assignment-item",
            b"UPDATE t SET a=0   ,b=1",
            b"0",
            1,
        ),
        (
            "completion/order-term",
            b"SELECT * FROM t ORDER BY a DESC NULLS LAST   ;",
            b"LAST",
            4,
        ),
        ("completion/limit", b"SELECT * FROM t LIMIT 1   ;", b"1", 1),
        (
            "completion/datatype",
            b"CREATE TABLE t(a VARCHAR(5)   ) STORAGE DISK",
            b"(5)",
            3,
        ),
        (
            "completion/definition-comma",
            b"CREATE TABLE t(a BIGINT,   b BIGINT) STORAGE DISK",
            b",",
            1,
        ),
        (
            "completion/index-key-comma",
            b"CREATE INDEX i ON t(a,   b)",
            b",",
            1,
        ),
        ("completion/drop-table", b"DROP TABLE t   ;", b"t", 1),
        ("completion/drop-index", b"DROP INDEX i   ;", b"i", 1),
    ];
    let selected = resource_case(|id| cases.iter().any(|case| case.0 == id));
    for &(id, source, token, advance) in cases {
        if !selected_case(&selected, id) {
            continue;
        }
        let at = locate(source, token) + advance;
        for timeout in [false, true] {
            let session = Quota::new(SESSION);
            let database = Quota::new(DATABASE);
            let mut parser = Parser::new(&session, &database);
            assert_eq!(
                control_failure(&mut parser, source, &database, at, timeout),
                error(
                    if timeout {
                        Code::Timeout
                    } else {
                        Code::Cancelled
                    },
                    Module::Parse,
                    point(at)
                )
            );
            usage(&session, &database, 0);
        }
    }
}

#[test]
fn remaining_interior_and_move_controls() {
    for source in [update(300), table(300, false)] {
        for timeout in [false, true] {
            let session = Quota::new(SESSION);
            let database = Quota::new(DATABASE);
            let mut parser = Parser::new(&session, &database);
            let token = Cancellation::default();
            let clock = TestClock::default();
            let seen = Cell::new(None);
            let observer = |module, span: Span| {
                if module == Module::Parse
                    && database.used() > source.len() as u64
                    && span.start >= 512
                    && span.start < source.len()
                {
                    seen.set(Some(span.start));
                    if timeout {
                        clock.0.set(10);
                    } else {
                        token.cancel();
                    }
                }
            };
            let error = parser
                .parse(
                    &source,
                    &CallControl::new(&token, &clock, Some(10)).with_observer(&observer),
                )
                .unwrap_err();
            let at = seen.get().expect("new statement list interior checkpoint");
            assert!((512..=768).contains(&at));
            assert_eq!(
                error,
                self::error(
                    if timeout {
                        Code::Timeout
                    } else {
                        Code::Cancelled
                    },
                    Module::Parse,
                    point(at)
                )
            );
            usage(&session, &database, 0);
        }
    }
    for timeout in [false, true] {
        let session = Quota::new(SESSION);
        let database = Quota::new(DATABASE);
        let mut parser = Parser::new(&session, &database);
        let source = update(5);
        let peak = 32 + 48 * node_bytes();
        let token = Cancellation::default();
        let clock = TestClock::default();
        let witnessed = Cell::new(false);
        let observer = |module, span: Span| {
            // The completed first assignment ends at16. It is moved while both
            // old16 and new32 arenas remain charged; growth began at token29.
            if module == Module::Parse && database.used() == peak && span == point(16) {
                assert_eq!(session.used(), peak);
                witnessed.set(true);
                if timeout {
                    clock.0.set(10);
                } else {
                    token.cancel();
                }
            }
        };
        assert_eq!(
            parser
                .parse(
                    &source,
                    &CallControl::new(&token, &clock, Some(10)).with_observer(&observer)
                )
                .unwrap_err(),
            error(
                if timeout {
                    Code::Timeout
                } else {
                    Code::Cancelled
                },
                Module::Parse,
                point(16)
            )
        );
        assert!(witnessed.get());
        usage(&session, &database, 0);
        drop(parse(&mut parser, SMALL).unwrap());
    }
}

#[test]
fn remaining_where_unwind_controls() {
    for prefix in [
        "SELECT * FROM t WHERE ",
        "UPDATE t SET a=0 WHERE ",
        "DELETE FROM t WHERE ",
    ] {
        let source = nested(prefix, 64);
        // Each closing Group advances one original byte after the innermost
        // comparison; this selects an unwind, rather than lexer/source copying.
        let at = prefix.len() + 100;
        for timeout in [false, true] {
            let session = Quota::new(SESSION);
            let database = Quota::new(DATABASE);
            let mut parser = Parser::new(&session, &database);
            assert_eq!(
                control_failure(&mut parser, &source, &database, at, timeout),
                error(
                    if timeout {
                        Code::Timeout
                    } else {
                        Code::Cancelled
                    },
                    Module::Parse,
                    point(at)
                )
            );
            usage(&session, &database, 0);
        }
    }
}

#[test]
fn remaining_ddl_numeric_controls() {
    let selected = resource_case(|id| matches!(id, "numeric/raw-length" | "numeric/exponent"));
    let session = Quota::new(SESSION);
    let database = Quota::new(DATABASE);
    let mut parser = Parser::new(&session, &database);
    if selected_case(&selected, "numeric/raw-length") {
        let source = format!(
            "CREATE TABLE t(a VARCHAR(-{})) STORAGE DISK",
            "0".repeat(4096)
        );
        let values = Cell::new(0);
        let observer = |module, _: Span| {
            if module == Module::Value {
                values.set(values.get() + 1);
            }
        };
        let token = Cancellation::default();
        let clock = TestClock::default();
        let tree = parser
            .parse(
                source.as_bytes(),
                &CallControl::new(&token, &clock, None).with_observer(&observer),
            )
            .unwrap();
        assert_eq!(
            values.get(),
            0,
            "raw DDL must not call scalar value validation"
        );
        let length = tree
            .nodes()
            .iter()
            .find_map(|node| match node.kind() {
                NodeKind::DataType(TypeSyntax::Utf8(length)) => Some(*length),
                _ => None,
            })
            .unwrap();
        assert_eq!(length.sign, Sign::Minus);
        assert_eq!(length.digits.end - length.digits.start, 4096);
        drop(tree);
        usage(&session, &database, 0);
    }
    if selected_case(&selected, "numeric/exponent") {
        // Only the Parse exponent classification pass is selected. Lex's earlier
        // scan and source copy cannot trigger this source-owning parser checkpoint.
        let source = format!(
            "CREATE TABLE t(a VARCHAR(1e{})) STORAGE DISK",
            "0".repeat(4096)
        );
        for timeout in [false, true] {
            let token = Cancellation::default();
            let clock = TestClock::default();
            let witnessed = Cell::new(None);
            let observer = |module, span: Span| {
                if module == Module::Parse
                    && database.used() > source.len() as u64
                    && (700..=956).contains(&span.start)
                {
                    witnessed.set(Some(span.start));
                    if timeout {
                        clock.0.set(10);
                    } else {
                        token.cancel();
                    }
                }
            };
            let result = parser.parse(
                source.as_bytes(),
                &CallControl::new(&token, &clock, Some(10)).with_observer(&observer),
            );
            let at = witnessed.get().expect("bounded DDL numeric-extension scan");
            assert_eq!(
                result.unwrap_err(),
                error(
                    if timeout {
                        Code::Timeout
                    } else {
                        Code::Cancelled
                    },
                    Module::Parse,
                    point(at)
                )
            );
            usage(&session, &database, 0);
        }
    }
}

#[test]
fn remaining_bounded_stack_and_drop() {
    assert!(size_of::<Parser>() <= 128);
    assert!(size_of::<ParsedStatement>() <= 384);
    assert!(size_of::<AstNode>() <= 256);
    let large_update = update(2729); // 4+3*2729 =8191.
    let max_table = table(2729, false); // 5+3*2729 =8192.
    let over_table = table(2729, true); // One extra prefix Name =8193.
    let overflow_at = locate(&over_table, b"STORAGE");
    let nested_inputs = [
        "SELECT * FROM t WHERE ",
        "UPDATE t SET a=0 WHERE ",
        "DELETE FROM t WHERE ",
    ]
    .map(|prefix| (prefix.len(), nested(prefix, 64), nested(prefix, 65)));
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || {
            let session = Quota::new(SESSION);
            let database = Quota::new(DATABASE);
            let mut parser = Parser::new(&session, &database);
            for (source, nodes) in [(large_update, 8191), (max_table, 8192)] {
                let tree = parse(&mut parser, &source).unwrap();
                assert_eq!(tree.nodes().len(), nodes);
                drop(tree);
                usage(&session, &database, 0);
            }
            assert_eq!(
                parse(&mut parser, &over_table).unwrap_err(),
                limit(overflow_at)
            );
            usage(&session, &database, 0);
            for (start, allowed, rejected) in nested_inputs {
                let tree = parse(&mut parser, &allowed).unwrap();
                drop(tree);
                assert_eq!(
                    parse(&mut parser, &rejected).unwrap_err(),
                    limit(start + 64)
                );
                usage(&session, &database, 0);
            }
            drop(parse(&mut parser, b"DROP TABLE t").unwrap());
            usage(&session, &database, 0);
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn remaining_shared_database() {
    let held = 15 + 16 * node_bytes();
    let new_peak = 32 + 48 * node_bytes();
    for delta in [-1_i64, 0, 1] {
        let database = Quota::new((held + new_peak).checked_add_signed(delta).unwrap());
        let (first_session, first) = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let session = Quota::new(SESSION);
                    let tree = parse(&mut Parser::new(&session, &database), SMALL).unwrap();
                    (session, tree)
                })
                .join()
                .unwrap()
        });
        let (second_session, result) = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let session = Quota::new(SESSION);
                    let result = parse(&mut Parser::new(&session, &database), &update(5));
                    (session, result)
                })
                .join()
                .unwrap()
        });
        if delta < 0 {
            assert_eq!(result.unwrap_err(), limit(29));
            assert_eq!(second_session.used(), 0);
        } else {
            let second = result.unwrap();
            assert_eq!(second_session.used(), 32 + 32 * node_bytes());
            assert_eq!(database.used(), held + charge(&second));
            drop(second);
        }
        assert_eq!(first.source(), SMALL);
        assert_eq!(first_session.used(), held);
        assert_eq!(database.used(), held);
        drop(first);
        assert_eq!(first_session.used(), 0);
        assert_eq!(database.used(), 0);
    }
}
