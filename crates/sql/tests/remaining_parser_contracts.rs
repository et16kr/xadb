//! Complete observations for P4's independently authored raw statement inputs.

use foundation::runtime::{Cancellation, Clock, Quota};
use sql::frontend::{
    CallControl, Code, ComparisonOp, Diagnostic, Module, NodeKind, NullPlacement,
    NullabilitySyntax, ParsedStatement, Parser, ScalarSyntax, Sign, SortDirection,
    TypeLengthSyntax, TypeSyntax,
};
use std::collections::BTreeSet;
use std::time::{Duration, Instant};
use test_reference::frontend::remaining_parser::{
    Case, Comparison, DataType, Direction, ErrorObservation, Kind, NameObservation, Node,
    NullPlacement as ObservedNullPlacement, Nullability, Observation, ScalarObservation,
    TypeLength, boundary_cases, changed_dispatch_cases, check, contextual_cases, diagnostic_cases,
    generated_cases, known_bad_observations, raw_payload_cases, structural_cases,
};

struct FixedClock;
impl Clock for FixedClock {
    fn now_nanos(&self) -> u64 {
        0
    }
}

fn sign(value: Sign) -> &'static str {
    match value {
        Sign::None => "None",
        Sign::Plus => "Plus",
        Sign::Minus => "Minus",
    }
}

fn length(value: TypeLengthSyntax) -> TypeLength {
    TypeLength {
        sign: sign(value.sign),
        digits: [value.digits.start, value.digits.end],
        span: [value.span.start, value.span.end],
    }
}

fn scalar(value: &ScalarSyntax) -> ScalarObservation {
    match value {
        ScalarSyntax::Integer {
            sign: value,
            digits,
        } => ScalarObservation::Integer {
            sign: sign(*value),
            digits: [digits.start, digits.end],
        },
        ScalarSyntax::Text => ScalarObservation::Text,
        ScalarSyntax::Binary => ScalarObservation::Binary,
        ScalarSyntax::Bool(value) => ScalarObservation::Bool(*value),
        ScalarSyntax::Null => ScalarObservation::Null,
        ScalarSyntax::Parameter { ordinal } => ScalarObservation::Parameter { ordinal: *ordinal },
    }
}

fn kind(value: &NodeKind) -> Kind {
    match value {
        NodeKind::Select => Kind::Select,
        NodeKind::Update => Kind::Update,
        NodeKind::Delete => Kind::Delete,
        NodeKind::CreateTable => Kind::CreateTable,
        NodeKind::DropTable => Kind::DropTable,
        NodeKind::CreateIndex { unique } => Kind::CreateIndex { unique: *unique },
        NodeKind::DropIndex => Kind::DropIndex,
        NodeKind::Projection => Kind::Projection,
        NodeKind::ProjectionItem => Kind::ProjectionItem,
        NodeKind::Wildcard => Kind::Wildcard,
        NodeKind::Assignments => Kind::Assignments,
        NodeKind::Assignment => Kind::Assignment,
        NodeKind::Where => Kind::Where,
        NodeKind::OrderBy => Kind::OrderBy,
        NodeKind::OrderTerm { direction, nulls } => Kind::OrderTerm {
            direction: direction.map(|value| match value {
                SortDirection::Asc => Direction::Asc,
                SortDirection::Desc => Direction::Desc,
            }),
            nulls: nulls.map(|value| match value {
                NullPlacement::First => ObservedNullPlacement::First,
                NullPlacement::Last => ObservedNullPlacement::Last,
            }),
        },
        NodeKind::Limit => Kind::Limit,
        NodeKind::TableDefinition => Kind::TableDefinition,
        NodeKind::ColumnDefinition { nullability } => Kind::ColumnDefinition {
            nullability: match nullability {
                NullabilitySyntax::Implicit => Nullability::Implicit,
                NullabilitySyntax::Null => Nullability::Null,
                NullabilitySyntax::NotNull => Nullability::NotNull,
            },
        },
        NodeKind::DataType(value) => Kind::DataType(match value {
            TypeSyntax::Int64 => DataType::Int64,
            TypeSyntax::Bool => DataType::Bool,
            TypeSyntax::Utf8(value) => DataType::Utf8(length(*value)),
            TypeSyntax::Binary(value) => DataType::Binary(length(*value)),
        }),
        NodeKind::PrimaryKey => Kind::PrimaryKey,
        NodeKind::UniqueConstraint => Kind::UniqueConstraint,
        NodeKind::StorageDisk => Kind::StorageDisk,
        NodeKind::Target => Kind::Target,
        NodeKind::Columns => Kind::Columns,
        NodeKind::Or => Kind::Or,
        NodeKind::And => Kind::And,
        NodeKind::Not => Kind::Not,
        NodeKind::Group => Kind::Group,
        NodeKind::Compare(value) => Kind::Compare(match value {
            ComparisonOp::Eq => Comparison::Eq,
            ComparisonOp::NotEq => Comparison::Ne,
            ComparisonOp::Lt => Comparison::Lt,
            ComparisonOp::Le => Comparison::Le,
            ComparisonOp::Gt => Comparison::Gt,
            ComparisonOp::Ge => Comparison::Ge,
        }),
        NodeKind::IsNull { negated } => Kind::IsNull { negated: *negated },
        NodeKind::Name(value) => Kind::Name(NameObservation {
            bytes: value.as_bytes().to_vec(),
            quoted: value.is_quoted(),
            span: [value.span().start, value.span().end],
        }),
        NodeKind::Scalar(value) => Kind::Scalar(scalar(value)),
        NodeKind::Insert
        | NodeKind::Begin
        | NodeKind::Commit
        | NodeKind::Rollback
        | NodeKind::SetAutocommit(_)
        | NodeKind::Rows
        | NodeKind::Tuple
        | NodeKind::Predicate => Kind::Unexpected(format!("{value:?}")),
    }
}

fn statement(value: &ParsedStatement) -> Observation {
    Observation {
        source: value.source().to_vec(),
        nodes: value
            .nodes()
            .iter()
            .map(|value| Node {
                kind: kind(value.kind()),
                span: [value.span().start, value.span().end],
                first_child: value.first_child(),
                next_sibling: value.next_sibling(),
            })
            .collect(),
        marker_count: value.marker_count(),
        error: None,
        complete: true,
    }
}

fn error(value: Diagnostic) -> ErrorObservation {
    ErrorObservation {
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
            Code::ParameterTypeUnknown => "ParameterTypeUnknown",
            Code::BindCount => "BindCount",
            Code::RowCount => "RowCount",
        },
        module: match value.module {
            Module::Lex => "Lex",
            Module::Name => "Name",
            Module::Split => "Split",
            Module::Parse => "Parse",
            Module::Value => "Value",
            Module::Compile => "Compile",
            Module::Eval => "Eval",
        },
        span: [value.span.start, value.span.end],
    }
}

fn assert_case(case: &Case) {
    let session = Quota::new(8 * 1024 * 1024);
    let database = Quota::new(64 * 1024 * 1024);
    let cancellation = Cancellation::default();
    let control = CallControl::new(&cancellation, &FixedClock, None);
    let mut parser = Parser::new(&session, &database);
    let observed = match parser.parse(&case.source, &control) {
        Ok(value) => statement(&value),
        Err(value) => Observation {
            source: Vec::new(),
            nodes: Vec::new(),
            marker_count: 0,
            error: Some(error(value)),
            complete: true,
        },
    };
    if !check(case, &observed) {
        let expected = &case.expected;
        let different = observed
            .nodes
            .iter()
            .zip(&expected.nodes)
            .position(|(actual, wanted)| actual != wanted)
            .unwrap_or(observed.nodes.len().min(expected.nodes.len()));
        panic!(
            "case={} nodes={}/{} first-difference={} actual={:?} expected={:?} source-equal={} markers={}/{} error={:?}/{:?} complete={}/{}",
            case.id,
            observed.nodes.len(),
            expected.nodes.len(),
            different,
            observed.nodes.get(different),
            expected.nodes.get(different),
            observed.source == expected.source,
            observed.marker_count,
            expected.marker_count,
            observed.error,
            expected.error,
            observed.complete,
            expected.complete,
        );
    }
    drop(parser);
    assert_eq!(session.used(), 0, "{} session cleanup", case.id);
    assert_eq!(database.used(), 0, "{} database cleanup", case.id);
}

fn assert_group(cases: Vec<Case>, family: &str) {
    let expected_count = match family {
        "structural" => 70,
        "raw" => 45,
        "contextual" => 95,
        "diagnostic" => 444,
        "dispatch" => 5,
        "boundary" => 129,
        "generated" => 8192,
        _ => panic!("unknown frozen family {family}"),
    };
    assert_eq!(cases.len(), expected_count, "frozen count for {family}");
    assert!(cases.iter().all(|case| case.family == family));
    let ids: BTreeSet<_> = cases.iter().map(|case| case.id.as_str()).collect();
    assert_eq!(
        ids.len(),
        cases.len(),
        "duplicate input identity in {family}"
    );
    // A recorded continuation can skip only the already completed prefix. An
    // unknown or invalid selector fails before any product call is made.
    let continuation = optional_selector("XADB_REMAINING_FROM_CASE");
    let exact = optional_selector("XADB_REMAINING_CASE");
    assert!(
        continuation.is_none() || exact.is_none(),
        "conflicting case selectors"
    );
    assert!(
        optional_selector("XADB_REMAINING_RESOURCE_CASE").is_none(),
        "resource selector on semantic target"
    );
    let selected = continuation.as_ref().or(exact.as_ref());
    let from = match selected {
        Some(id) => cases
            .iter()
            .position(|case| &case.id == id)
            .unwrap_or_else(|| panic!("unknown frozen case selector {id} in {family}")),
        None => 0,
    };
    let end = if exact.is_some() {
        from + 1
    } else {
        cases.len()
    };
    println!(
        "family={family} inventory={} skipped-prefix={from} selected={} exact={}",
        cases.len(),
        end - from,
        exact.is_some()
    );
    for case in &cases[from..end] {
        assert_case(case);
    }
    println!("family={family} completed={}", end - from);
}

fn optional_selector(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(value) => Some(value),
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(_)) => panic!("non-Unicode selector {name}"),
    }
}

#[test]
fn remaining_statement_shapes() {
    assert_group(structural_cases(), "structural");
}

#[test]
fn remaining_raw_payloads() {
    assert_group(raw_payload_cases(), "raw");
}

#[test]
fn remaining_contextual_names() {
    assert_group(contextual_cases(), "contextual");
}

#[test]
fn remaining_diagnostics() {
    assert_group(diagnostic_cases(), "diagnostic");
}

#[test]
fn remaining_boundaries() {
    assert_group(boundary_cases(), "boundary");
}

#[test]
fn remaining_changed_dispatch() {
    let cases = changed_dispatch_cases();
    assert_eq!(cases.len(), 5);
    assert_group(cases, "dispatch");
}

#[test]
fn remaining_generated_statements() {
    let mut cases = Vec::new();
    for seed in 0..64 {
        let next = generated_cases(seed, 128);
        assert_eq!(next.len(), 128);
        cases.extend(next);
    }
    assert_eq!(cases.len(), 8192);
    assert_group(cases, "generated");
}

#[test]
fn remaining_known_bad_observations() {
    // This metadata-only audit includes the complete fixed inventory even when
    // an independently recorded product continuation selects a later case.
    let mut fixed_ids = BTreeSet::new();
    for cases in [
        structural_cases(),
        raw_payload_cases(),
        contextual_cases(),
        diagnostic_cases(),
        changed_dispatch_cases(),
        boundary_cases(),
    ] {
        for case in cases {
            assert!(fixed_ids.insert(case.id), "duplicate fixed input identity");
        }
    }
    assert_eq!(fixed_ids.len(), 788);
    let mutations = known_bad_observations();
    assert_eq!(mutations.len(), 50);
    let mut ids = BTreeSet::new();
    let mut controls = BTreeSet::new();
    for mutation in mutations {
        assert!(ids.insert(mutation.id), "duplicate known-bad identity");
        if controls.insert(mutation.case.id.clone()) {
            assert!(check(&mutation.case, &mutation.case.expected));
        }
        assert!(
            !check(&mutation.case, &mutation.observed),
            "accepted known-bad {} for {}",
            mutation.id,
            mutation.case.id,
        );
    }
    assert_eq!(ids.len(), 50);
    assert_eq!(controls.len(), 12);
    println!(
        "known-bad={} controls={} product-calls=0",
        ids.len(),
        controls.len()
    );
}

#[test]
#[ignore = "Separately selected P4 expansion: 256 seeds x 1024 statements"]
fn remaining_extended_generated_statements() {
    for seed in 0..256 {
        let cases = generated_cases(seed, 1024);
        assert_eq!(cases.len(), 1024);
        for case in cases {
            assert_case(&case);
        }
    }
}

#[test]
#[ignore = "Separately selected 30-minute valid-grammar profile, not arbitrary-byte fuzz"]
fn remaining_extended_grammar_30_minutes() {
    let start = Instant::now();
    let mut seed = 256_u64;
    while start.elapsed() < Duration::from_secs(1800) {
        let cases = generated_cases(seed, 128);
        assert_eq!(cases.len(), 128);
        for case in cases {
            assert_case(&case);
        }
        seed = seed
            .checked_add(1)
            .expect("bounded generated seed sequence");
    }
    assert!(seed > 256);
    println!(
        "P4 generated grammar seeds=256..{} elapsed={:?}",
        seed - 1,
        start.elapsed()
    );
}
