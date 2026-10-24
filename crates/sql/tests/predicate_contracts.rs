use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};
use xadb_foundation::runtime::{Cancellation, Clock, Quota};
use xadb_sql::frontend::{
    CallControl, Code, ComparisonOp, Diagnostic, Module, NodeKind, ParsedStatement, Parser,
    ScalarSyntax, Sign,
};
use xadbtest_reference::frontend::parser::{ErrorObservation, NameObservation, ScalarObservation};
use xadbtest_reference::frontend::predicate::{
    Case, Comparison, Kind, Node, Observation, boundary_cases, check, error_cases, generated_case,
    known_bad_observations, literal_shapes, raw_value_cases,
};

struct FixedClock;

impl Clock for FixedClock {
    fn now_nanos(&self) -> u64 {
        0
    }
}

fn scalar_observation(value: &ScalarSyntax) -> ScalarObservation {
    match value {
        ScalarSyntax::Integer { sign, digits } => ScalarObservation::Integer {
            sign: match sign {
                Sign::None => "None",
                Sign::Plus => "Plus",
                Sign::Minus => "Minus",
            },
            digits: [digits.start, digits.end],
        },
        ScalarSyntax::Text => ScalarObservation::Text,
        ScalarSyntax::Binary => ScalarObservation::Binary,
        ScalarSyntax::Bool(value) => ScalarObservation::Bool(*value),
        ScalarSyntax::Null => ScalarObservation::Null,
        ScalarSyntax::Parameter { ordinal } => ScalarObservation::Parameter { ordinal: *ordinal },
    }
}

fn comparison_observation(value: &ComparisonOp) -> Comparison {
    match value {
        ComparisonOp::Eq => Comparison::Eq,
        ComparisonOp::NotEq => Comparison::Ne,
        ComparisonOp::Lt => Comparison::Lt,
        ComparisonOp::Le => Comparison::Le,
        ComparisonOp::Gt => Comparison::Gt,
        ComparisonOp::Ge => Comparison::Ge,
    }
}

fn statement_observation(statement: &ParsedStatement) -> Observation {
    let nodes = statement
        .nodes()
        .iter()
        .map(|value| Node {
            kind: match value.kind() {
                NodeKind::Predicate => Kind::Predicate,
                NodeKind::Or => Kind::Or,
                NodeKind::And => Kind::And,
                NodeKind::Not => Kind::Not,
                NodeKind::Group => Kind::Group,
                NodeKind::Compare(op) => Kind::Compare(comparison_observation(op)),
                NodeKind::IsNull { negated } => Kind::IsNull { negated: *negated },
                NodeKind::Name(name) => Kind::Name(NameObservation {
                    bytes: name.as_bytes().to_vec(),
                    quoted: name.is_quoted(),
                    span: [name.span().start, name.span().end],
                }),
                NodeKind::Scalar(scalar) => Kind::Scalar(scalar_observation(scalar)),
                other => panic!("predicate fragment returned non-predicate node: {other:?}"),
            },
            span: [value.span().start, value.span().end],
            first_child: value.first_child(),
            next_sibling: value.next_sibling(),
        })
        .collect();
    Observation {
        source: statement.source().to_vec(),
        nodes,
        marker_count: statement.marker_count(),
        error: None,
        complete: true,
    }
}

fn error_observation(error: Diagnostic) -> ErrorObservation {
    ErrorObservation {
        code: match error.code {
            Code::Syntax => "Syntax",
            Code::Unsupported => "Unsupported",
            Code::Name => "Name",
            Code::Limit => "Limit",
            Code::Cancelled => "Cancelled",
            Code::Timeout => "Timeout",
            Code::ParameterTypeUnknown => "ParameterTypeUnknown",
            Code::BindCount => "BindCount",
            Code::RowCount => "RowCount",
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
            Module::Compile => "Compile",
            Module::Eval => "Eval",
        },
        span: [error.span.start, error.span.end],
    }
}

fn assert_case(case: &Case) {
    let session = Quota::new(8 * 1024 * 1024);
    let database = Quota::new(64 * 1024 * 1024);
    let cancellation = Cancellation::default();
    let control = CallControl::new(&cancellation, &FixedClock, None);
    let mut parser = Parser::new(&session, &database);
    let observed = match parser.parse_predicate(&case.source, &control) {
        Ok(statement) => statement_observation(&statement),
        Err(error) => Observation {
            source: Vec::new(),
            nodes: Vec::new(),
            marker_count: 0,
            error: Some(error_observation(error)),
            complete: true,
        },
    };
    if !check(case, &observed) {
        let expected = &case.expected;
        let node_difference = observed
            .nodes
            .iter()
            .zip(&expected.nodes)
            .position(|(actual, wanted)| actual != wanted)
            .unwrap_or(observed.nodes.len().min(expected.nodes.len()));
        let source_difference = observed
            .source
            .iter()
            .zip(&expected.source)
            .position(|(actual, wanted)| actual != wanted)
            .unwrap_or(observed.source.len().min(expected.source.len()));
        panic!(
            "case={} nodes={}/{} first-node={} actual={:?} expected={:?} source-bytes={}/{} first-source-byte={} markers={}/{} error={:?}/{:?} complete={}/{}",
            case.id,
            observed.nodes.len(),
            expected.nodes.len(),
            node_difference,
            observed.nodes.get(node_difference),
            expected.nodes.get(node_difference),
            observed.source.len(),
            expected.source.len(),
            source_difference,
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

fn assert_family(name: &str, cases: Vec<Case>, count: usize) {
    assert_eq!(cases.len(), count, "{name} immutable fixture denominator");
    let mut ids = BTreeSet::new();
    for case in cases {
        assert!(
            ids.insert(case.id.clone()),
            "duplicate {name} case {}",
            case.id
        );
        assert_case(&case);
    }
    assert_eq!(ids.len(), count);
    println!("predicate semantic function={name} completed_calls={count} full_arrays=true");
}

#[test]
fn literal_predicate_precedence_and_shapes() {
    assert_family(
        "literal_predicate_precedence_and_shapes",
        literal_shapes(),
        35,
    );
}

#[test]
fn predicate_raw_value_and_marker_syntax() {
    assert_family(
        "predicate_raw_value_and_marker_syntax",
        raw_value_cases(),
        137,
    );
}

#[test]
fn predicate_syntax_and_unsupported_diagnostics() {
    assert_family(
        "predicate_syntax_and_unsupported_diagnostics",
        error_cases(),
        128,
    );
}

#[test]
fn predicate_depth_node_and_marker_boundaries() {
    let cases = boundary_cases();
    for (prefix, count) in [
        ("depth-", 11),
        ("nodes-", 6),
        ("markers-", 3),
        ("source-", 3),
        ("name-", 6),
    ] {
        assert_eq!(
            cases
                .iter()
                .filter(|case| case.id.starts_with(prefix))
                .count(),
            count,
            "required {prefix} boundary family",
        );
    }
    assert_family("predicate_depth_node_and_marker_boundaries", cases, 29);
    println!(
        "predicate boundaries depth=11 node=6 marker=3 source=3 name=6; all L-1/L/L+1 cells complete"
    );
}

fn assert_generated_case(seed: u64) {
    let case = generated_case(seed, 128);
    assert!(case.expected.error.is_none());
    assert_eq!(
        case.expected
            .nodes
            .iter()
            .filter(|node| matches!(node.kind, Kind::Compare(_) | Kind::IsNull { .. }))
            .count(),
        128,
        "seed={seed} must retain every chosen fragment",
    );
    assert_case(&case);
}

#[test]
fn generated_predicate_fragments() {
    let mut completed = 0;
    for seed in 0..64 {
        assert_generated_case(seed);
        completed += 1;
    }
    assert_eq!(completed, 64);
    assert_eq!(completed * 128, 8192);
    println!(
        "predicate generated completed_calls=64 completed_fragments=8192 seeds=0..63; all source bytes and preorder entries compared"
    );
}

#[test]
fn known_bad_predicate_observations_are_rejected() {
    let mutations = known_bad_observations();
    assert_eq!(mutations.len(), 36);
    let mut mutation_ids = BTreeSet::new();
    let mut controls = BTreeMap::new();
    for mutation in mutations {
        assert!(
            mutation_ids.insert(mutation.id),
            "duplicate mutant {}",
            mutation.id
        );
        assert_ne!(
            mutation.case.expected, mutation.observed,
            "{} is a no-op corruption",
            mutation.id,
        );
        match controls.entry(mutation.case.id.clone()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                assert!(
                    check(&mutation.case, &mutation.case.expected),
                    "{} rejected exact acceptance control",
                    mutation.case.id,
                );
                entry.insert((mutation.case.source.clone(), mutation.case.expected.clone()));
            }
            std::collections::btree_map::Entry::Occupied(entry) => {
                assert_eq!(
                    entry.get().0,
                    mutation.case.source,
                    "control source identity"
                );
                assert_eq!(
                    entry.get().1,
                    mutation.case.expected,
                    "control expected identity"
                );
            }
        }
        assert!(
            !check(&mutation.case, &mutation.observed),
            "{} accepted known-bad observation for {}",
            mutation.id,
            mutation.case.id,
        );
    }
    assert_eq!(mutation_ids.len(), 36);
    assert_eq!(controls.len(), 8);
    println!(
        "predicate checker rejected_corruptions=36 exact_deduplicated_controls=8; no product mutant execution claimed"
    );
}

#[test]
#[ignore = "Separately selected expansion: 256 seeds x 1024 fragments, eight full 128-fragment calls per seed"]
fn extended_generated_predicate_fragments() {
    let mut completed = 0;
    for seed in 0..256 {
        let mut seed_fragments = 0;
        for part in 0..8 {
            assert_generated_case(seed + 256 * part);
            completed += 1;
            seed_fragments += 128;
        }
        assert_eq!(
            seed_fragments, 1024,
            "seed={seed} full expansion denominator"
        );
    }
    assert_eq!(completed, 2048);
    assert_eq!(completed * 128, 262144);
    println!(
        "predicate expanded completed_calls=2048 completed_fragments=262144 seeds=0..255 parts=8; required seeds included, separate product-limit boundaries unchanged"
    );
}

#[test]
#[ignore = "Separately selected 30-minute bounded grammar-valid generation; not arbitrary malformed-byte fuzz"]
fn extended_predicate_generated_30_minutes() {
    let started = Instant::now();
    let mut seed = 2048_u64;
    let mut completed = 0_u64;
    while started.elapsed() < Duration::from_secs(1800) {
        assert_generated_case(seed);
        completed += 1;
        seed = seed
            .checked_add(1)
            .expect("bounded generated seed sequence");
    }
    assert!(completed > 0, "generated profile completed no case");
    println!(
        "predicate grammar-valid generated completed_calls={completed} completed_fragments={} seeds=2048..{} elapsed={:?}; arbitrary malformed bytes and exhaustive coverage are not claimed",
        completed * 128,
        seed - 1,
        started.elapsed(),
    );
}
