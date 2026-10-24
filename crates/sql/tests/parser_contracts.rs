use std::collections::BTreeSet;
use std::time::{Duration, Instant};
use xadb_foundation::runtime::{Cancellation, Clock, Quota};
use xadb_sql::frontend::{
    CallControl, Code, Diagnostic, Module, NodeKind, ParsedStatement, Parser, ScalarSyntax, Sign,
};
use xadbtest_reference::frontend::parser::{
    ErrorObservation, KindObservation, NameObservation, NodeObservation, ParseCase,
    ParseObservation, ScalarObservation, boundary_cases, check_parse, error_literals,
    generated_case, known_bad_observations, scalar_literals, structural_literals,
};

struct FixedClock;
impl Clock for FixedClock {
    fn now_nanos(&self) -> u64 {
        0
    }
}

fn scalar_observation(scalar: &ScalarSyntax) -> ScalarObservation {
    match scalar {
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

fn statement_observation(statement: &ParsedStatement) -> ParseObservation {
    let nodes = statement
        .nodes()
        .iter()
        .map(|value| NodeObservation {
            kind: match value.kind() {
                NodeKind::Insert => KindObservation::Insert,
                NodeKind::Begin => KindObservation::Begin,
                NodeKind::Commit => KindObservation::Commit,
                NodeKind::Rollback => KindObservation::Rollback,
                NodeKind::SetAutocommit(enabled) => KindObservation::SetAutocommit(*enabled),
                NodeKind::Target => KindObservation::Target,
                NodeKind::Columns => KindObservation::Columns,
                NodeKind::Rows => KindObservation::Rows,
                NodeKind::Tuple => KindObservation::Tuple,
                NodeKind::Name(name) => KindObservation::Name(NameObservation {
                    bytes: name.as_bytes().to_vec(),
                    quoted: name.is_quoted(),
                    span: [name.span().start, name.span().end],
                }),
                NodeKind::Scalar(scalar) => KindObservation::Scalar(scalar_observation(scalar)),
                NodeKind::Predicate
                | NodeKind::Or
                | NodeKind::And
                | NodeKind::Not
                | NodeKind::Group
                | NodeKind::Compare(_)
                | NodeKind::IsNull { .. } => panic!("predicate node in an INSERT/transaction tree"),
            },
            span: [value.span().start, value.span().end],
            first_child: value.first_child(),
            next_sibling: value.next_sibling(),
        })
        .collect();
    ParseObservation {
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

fn assert_case(case: &ParseCase) {
    let session = Quota::new(8 * 1024 * 1024);
    let database = Quota::new(64 * 1024 * 1024);
    let cancellation = Cancellation::default();
    let control = CallControl::new(&cancellation, &FixedClock, None);
    let mut parser = Parser::new(&session, &database);
    let observed = match parser.parse(&case.source, &control) {
        Ok(statement) => statement_observation(&statement),
        Err(error) => ParseObservation {
            source: Vec::new(),
            nodes: Vec::new(),
            marker_count: 0,
            error: Some(error_observation(error)),
            complete: true,
        },
    };
    if !check_parse(case, &observed) {
        let expected = &case.expected;
        let first_node_difference = observed
            .nodes
            .iter()
            .zip(&expected.nodes)
            .position(|(actual, wanted)| actual != wanted)
            .unwrap_or(observed.nodes.len().min(expected.nodes.len()));
        let first_source_difference = observed
            .source
            .iter()
            .zip(&expected.source)
            .position(|(actual, wanted)| actual != wanted)
            .unwrap_or(observed.source.len().min(expected.source.len()));
        panic!(
            "case={} node-counts={}/{} first-node-difference={} actual={:?} expected={:?} source-lengths={}/{} first-source-difference={} markers={}/{} errors={:?}/{:?} complete={}/{}",
            case.id,
            observed.nodes.len(),
            expected.nodes.len(),
            first_node_difference,
            observed.nodes.get(first_node_difference),
            expected.nodes.get(first_node_difference),
            observed.source.len(),
            expected.source.len(),
            first_source_difference,
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

#[test]
fn literal_statement_shapes() {
    let cases = structural_literals();
    assert_eq!(cases.len(), 24);
    for case in cases {
        assert_case(&case);
    }
}

#[test]
fn literal_raw_scalars() {
    let cases = scalar_literals();
    assert_eq!(cases.len(), 26);
    for case in cases {
        assert_case(&case);
    }
}

#[test]
fn literal_rejections() {
    let cases = error_literals();
    assert_eq!(cases.len(), 122);
    for case in cases {
        assert_case(&case);
    }
}

#[test]
fn parser_boundary_cases() {
    let cases = boundary_cases();
    assert_eq!(cases.len(), 22);
    for case in cases {
        assert_case(&case);
    }
}

#[test]
fn generated_scalar_fragments() {
    for seed in 0..64 {
        assert_case(&generated_case(seed, 128));
    }
}

#[test]
fn known_bad_parser_observations_are_rejected() {
    let mutations = known_bad_observations();
    assert_eq!(mutations.len(), 23);
    let mut accepted_controls = BTreeSet::new();
    for mutation in mutations {
        if accepted_controls.insert(mutation.case.id.clone()) {
            assert!(
                check_parse(&mutation.case, &mutation.case.expected),
                "{} rejected exact acceptance control",
                mutation.case.id,
            );
        }
        assert!(
            !check_parse(&mutation.case, &mutation.observed),
            "{} accepted known-bad observation for {}",
            mutation.id,
            mutation.case.id,
        );
    }
    assert_eq!(accepted_controls.len(), 4);
}

#[test]
#[ignore = "Separately selected P2 expansion: 256 seeds x 1024 raw scalar fragments"]
fn extended_generated_scalar_fragments() {
    for seed in 0..256 {
        assert_case(&generated_case(seed, 1024));
    }
}

#[test]
#[ignore = "Separately selected 30-minute valid-grammar generated profile; not arbitrary-byte fuzz"]
fn extended_parser_fuzz_30_minutes() {
    let started = Instant::now();
    let mut seed = 256_u64;
    while started.elapsed() < Duration::from_secs(1800) {
        let fragments = 1 + (seed % 1024) as usize;
        assert_case(&generated_case(seed, fragments));
        seed = seed
            .checked_add(1)
            .expect("bounded generated seed sequence");
    }
    assert!(seed > 256, "generated profile completed no case");
    println!(
        "P2 valid-grammar generated profile: completed={} seeds=256..{} elapsed={:?}; arbitrary malformed bytes and exhaustive coverage are not claimed",
        seed - 256,
        seed - 1,
        started.elapsed(),
    );
}
