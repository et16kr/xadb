//! Complete frozen observations for the new typed compile/evaluate consumer.

use foundation::runtime::{Cancellation, Clock, Quota};
use sql::frontend::{
    CallControl, Code, ColumnSpec, ComparisonOp, CompiledKindRef, CompiledPredicate, DeclaredType,
    Diagnostic, ExpressionType, Module, Parser, PredicateCompiler, ScalarRef, ScalarSpec,
    ScalarType, Span, Token, TokenKind, TruthValue, normalize_name,
};
use std::collections::BTreeSet;
use std::time::{Duration, Instant};
use test_reference::frontend::typed_predicate::{
    Case, Column, Comparison, ErrorObservation, Expression, Kind, Node, Observation, Parameter,
    Payload, Plan, Spec, Truth, TypeTag, ValueObservation, check, generated_cases,
    known_bad_observations, literal_cases,
};

struct FixedClock;
impl Clock for FixedClock {
    fn now_nanos(&self) -> u64 {
        0
    }
}

fn scalar_type(value: TypeTag) -> ScalarType {
    match value {
        TypeTag::Int64 => ScalarType::Int64,
        TypeTag::Bool => ScalarType::Bool,
        TypeTag::Utf8 => ScalarType::Utf8,
        TypeTag::Binary => ScalarType::Binary,
    }
}
fn observed_type(value: ScalarType) -> TypeTag {
    match value {
        ScalarType::Int64 => TypeTag::Int64,
        ScalarType::Bool => TypeTag::Bool,
        ScalarType::Utf8 => TypeTag::Utf8,
        ScalarType::Binary => TypeTag::Binary,
    }
}
fn input_spec(value: Spec) -> ScalarSpec {
    ScalarSpec {
        declared_type: match value.logical_type {
            TypeTag::Int64 => DeclaredType::Int64,
            TypeTag::Bool => DeclaredType::Bool,
            TypeTag::Utf8 => DeclaredType::Utf8(value.maximum_bytes.expect("explicit byte bound")),
            TypeTag::Binary => {
                DeclaredType::Binary(value.maximum_bytes.expect("explicit byte bound"))
            }
        },
        nullable: value.nullable,
    }
}
fn observed_spec(value: ScalarSpec) -> Spec {
    let (logical_type, maximum_bytes) = match value.declared_type {
        DeclaredType::Int64 => (TypeTag::Int64, None),
        DeclaredType::Bool => (TypeTag::Bool, None),
        DeclaredType::Utf8(n) => (TypeTag::Utf8, Some(n)),
        DeclaredType::Binary(n) => (TypeTag::Binary, Some(n)),
    };
    Spec {
        logical_type,
        maximum_bytes,
        nullable: value.nullable,
    }
}
fn input_ref(value: &ValueObservation) -> ScalarRef<'_> {
    match &value.payload {
        Payload::Null => ScalarRef::Null(scalar_type(value.logical_type)),
        Payload::Int64(v) => {
            assert_eq!(value.logical_type, TypeTag::Int64);
            ScalarRef::Int64(*v)
        }
        Payload::Bool(v) => {
            assert_eq!(value.logical_type, TypeTag::Bool);
            ScalarRef::Bool(*v)
        }
        Payload::Utf8(v) => {
            assert_eq!(value.logical_type, TypeTag::Utf8);
            ScalarRef::Utf8(std::str::from_utf8(v).expect("literal valid UTF8 input"))
        }
        Payload::Binary(v) => {
            assert_eq!(value.logical_type, TypeTag::Binary);
            ScalarRef::Binary(v)
        }
    }
}
fn observed_value(value: ScalarRef<'_>) -> ValueObservation {
    let (logical_type, payload) = match value {
        ScalarRef::Null(tag) => (observed_type(tag), Payload::Null),
        ScalarRef::Int64(v) => (TypeTag::Int64, Payload::Int64(v)),
        ScalarRef::Bool(v) => (TypeTag::Bool, Payload::Bool(v)),
        ScalarRef::Utf8(v) => (TypeTag::Utf8, Payload::Utf8(v.as_bytes().to_vec())),
        ScalarRef::Binary(v) => (TypeTag::Binary, Payload::Binary(v.to_vec())),
    };
    ValueObservation {
        logical_type,
        payload,
    }
}
fn span(value: Span) -> [usize; 2] {
    [value.start, value.end]
}
fn comparison(value: ComparisonOp) -> Comparison {
    match value {
        ComparisonOp::Eq => Comparison::Eq,
        ComparisonOp::NotEq => Comparison::Ne,
        ComparisonOp::Lt => Comparison::Lt,
        ComparisonOp::Le => Comparison::Le,
        ComparisonOp::Gt => Comparison::Gt,
        ComparisonOp::Ge => Comparison::Ge,
    }
}
fn observed_plan(value: &CompiledPredicate) -> Plan {
    Plan {
        span: span(value.span()),
        nodes: value
            .nodes()
            .iter()
            .map(|n| Node {
                kind: match n.kind() {
                    CompiledKindRef::Predicate => Kind::Predicate,
                    CompiledKindRef::Or => Kind::Or,
                    CompiledKindRef::And => Kind::And,
                    CompiledKindRef::Not => Kind::Not,
                    CompiledKindRef::Group => Kind::Group,
                    CompiledKindRef::Compare { op, scalar_type } => Kind::Compare {
                        op: comparison(op),
                        scalar_type: scalar_type.map(observed_type),
                    },
                    CompiledKindRef::IsNull { negated } => Kind::IsNull { negated },
                    CompiledKindRef::Column { ordinal } => Kind::Column { ordinal },
                    CompiledKindRef::Literal(value) => Kind::Literal(observed_value(value)),
                    CompiledKindRef::Null => Kind::Null,
                    CompiledKindRef::Parameter { ordinal } => Kind::Parameter { ordinal },
                },
                result_type: match n.result_type() {
                    ExpressionType::Truth => Expression::Truth,
                    ExpressionType::Scalar(tag) => Expression::Scalar(observed_type(tag)),
                    ExpressionType::UntypedNull => Expression::UntypedNull,
                },
                span: span(n.span()),
                first_child: n.first_child(),
                next_sibling: n.next_sibling(),
            })
            .collect(),
        columns: value
            .columns()
            .iter()
            .map(|c| Column {
                name: c.name_bytes().to_vec(),
                quoted: c.is_quoted(),
                spec: observed_spec(c.spec()),
                first_use: c.first_use().map(span),
            })
            .collect(),
        parameters: value
            .parameters()
            .iter()
            .map(|p| Parameter {
                ordinal: p.ordinal(),
                spec: observed_spec(p.spec()),
                span: span(p.span()),
            })
            .collect(),
    }
}
fn error(value: Diagnostic) -> ErrorObservation {
    ErrorObservation {
        code: match value.code {
            Code::Syntax => "Syntax",
            Code::Unsupported => "Unsupported",
            Code::Name => "Name",
            Code::Limit => "Limit",
            Code::Type => "Type",
            Code::Range => "Range",
            Code::NotNull => "NotNull",
            Code::Cancelled => "Cancelled",
            Code::Timeout => "Timeout",
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
        span: span(value.span),
    }
}

fn assert_case(case: &Case) {
    let session = Quota::new(8 << 20);
    let database = Quota::new(64 << 20);
    let cancellation = Cancellation::default();
    let control = CallControl::new(&cancellation, &FixedClock, None);
    let mut parser = Parser::new(&session, &database);
    let parsed = parser
        .parse_predicate(&case.source, &control)
        .unwrap_or_else(|error| panic!("{} typed input failed raw parser: {error:?}", case.id));
    let names: Vec<_> = case
        .columns
        .iter()
        .map(|c| {
            normalize_name(
                &c.raw_name,
                Token {
                    kind: if c.raw_name.first() == Some(&b'"') {
                        TokenKind::QuotedIdentifier
                    } else {
                        TokenKind::Identifier
                    },
                    span: Span {
                        start: 0,
                        end: c.raw_name.len(),
                    },
                },
                &control,
            )
            .expect("explicit validated schema name")
        })
        .collect();
    let columns: Vec<_> = names
        .iter()
        .zip(&case.columns)
        .map(|(name, c)| ColumnSpec {
            name,
            spec: input_spec(c.spec),
        })
        .collect();
    let hints = case
        .hints
        .as_ref()
        .map(|v| v.iter().map(|v| v.map(scalar_type)).collect::<Vec<_>>());
    let mut compiler = PredicateCompiler::new(&session, &database);
    let mut observation = Observation {
        plan: None,
        compile_error: None,
        evaluation_attempted: false,
        truth: None,
        where_keep: None,
        eval_error: None,
        complete: true,
    };
    match compiler.compile(&parsed, &columns, hints.as_deref(), &control) {
        Ok(compiled) => {
            observation.plan = Some(observed_plan(&compiled));
            if case.evaluate {
                let row: Vec<_> = case.row.iter().map(input_ref).collect();
                let parameters: Vec<_> = case.parameters.iter().map(input_ref).collect();
                observation.evaluation_attempted = true;
                match compiled.evaluate(&row, &parameters, &control) {
                    Ok(truth) => {
                        observation.truth = Some(match truth {
                            TruthValue::True => Truth::T,
                            TruthValue::False => Truth::F,
                            TruthValue::Unknown => Truth::U,
                        });
                        observation.where_keep = Some(truth.passes_where());
                    }
                    Err(diagnostic) => observation.eval_error = Some(error(diagnostic)),
                }
            }
        }
        Err(diagnostic) => observation.compile_error = Some(error(diagnostic)),
    }
    if !check(case, &observation) {
        let first_difference = observation
            .plan
            .as_ref()
            .zip(case.expected.plan.as_ref())
            .map(|(a, e)| {
                a.nodes
                    .iter()
                    .zip(&e.nodes)
                    .position(|(a, e)| a != e)
                    .unwrap_or(a.nodes.len().min(e.nodes.len()))
            });
        panic!(
            "case={} full-observation mismatch: compile={:?}/{:?}, eval={:?}/{:?}, truth={:?}/{:?}, where={:?}/{:?}, first-node={:?}, node-count={:?}/{:?}, columns-equal={}, parameters-equal={}",
            case.id,
            observation.compile_error,
            case.expected.compile_error,
            observation.eval_error,
            case.expected.eval_error,
            observation.truth,
            case.expected.truth,
            observation.where_keep,
            case.expected.where_keep,
            first_difference,
            observation.plan.as_ref().map(|p| p.nodes.len()),
            case.expected.plan.as_ref().map(|p| p.nodes.len()),
            observation.plan.as_ref().map(|p| &p.columns)
                == case.expected.plan.as_ref().map(|p| &p.columns),
            observation.plan.as_ref().map(|p| &p.parameters)
                == case.expected.plan.as_ref().map(|p| &p.parameters)
        );
    }
    drop(parsed);
    drop(parser);
    drop(compiler);
    assert_eq!(session.used(), 0, "{} session cleanup", case.id);
    assert_eq!(database.used(), 0, "{} database cleanup", case.id);
}

fn group(family: &str) -> usize {
    match family {
        "truth_not" | "truth_and" | "truth_or" | "where" | "same_type" | "static_cross_type"
        | "runtime_tag" | "is_null" | "raw_null_context" => 0,
        "byte_order" | "trailing_origin" | "empty_origin" | "origin_is_null"
        | "empty_static_type" => 1,
        "inference"
        | "hint_count"
        | "hint_conflict"
        | "parameter_nullable"
        | "parameter_length_independent"
        | "unknown_marker"
        | "hint_resolves_null"
        | "pair_hint"
        | "parameter_count"
        | "pair_hint_conflict"
        | "occurrence_independence"
        | "parameter_length"
        | "max_parameter_hint_count"
        | "max_parameter_value_count" => 2,
        "compiled_constant_boundary"
        | "metadata_length"
        | "row_not_null"
        | "row_length"
        | "row_count"
        | "schema_count"
        | "max_schema_reference" => 3,
        "decode_all"
        | "decode_order"
        | "input_priority"
        | "leading_trivia"
        | "name_identity"
        | "phase_precedence"
        | "priority_positions"
        | "runtime_priority"
        | "static_before_decode"
        | "static_priority"
        | "whole_validation_guards" => 4,
        other => panic!("unregistered canonical family {other}"),
    }
}
fn assert_group(selected: usize, count: usize, evals: usize) {
    let cases = literal_cases();
    assert_eq!(cases.len(), 946);
    let mut identities = BTreeSet::new();
    for case in &cases {
        assert!(identities.insert(case.id.clone()), "duplicate canonical ID");
    }
    let selected: Vec<_> = cases
        .iter()
        .filter(|c| group(c.family) == selected)
        .collect();
    assert_eq!(selected.len(), count);
    assert_eq!(
        selected
            .iter()
            .filter(|c| c.expected.evaluation_attempted)
            .count(),
        evals
    );
    // Resume a failed group without repeating its completed, unchanged prefix.
    // An unknown ID is a selection error, never an empty successful run.
    let first = match std::env::var("XADB_TYPED_FROM_CASE") {
        Ok(id) => selected
            .iter()
            .position(|case| case.id == id)
            .unwrap_or_else(|| panic!("resume case {id} is absent from this group")),
        Err(std::env::VarError::NotPresent) => 0,
        Err(std::env::VarError::NotUnicode(_)) => panic!("resume case ID is not valid UTF-8"),
    };
    let selected = &selected[first..];
    let selected_count = selected.len();
    let selected_evals = selected
        .iter()
        .filter(|case| case.expected.evaluation_attempted)
        .count();
    assert!(selected_count > 0);
    for case in selected {
        assert_case(case);
    }
    eprintln!(
        "typed canonical_cases={selected_count} compile_calls={selected_count} eval_calls={selected_evals} full_arrays=true group_inventory={count} completed_prefix_skipped={first}"
    );
}

#[test]
fn typed_truth_and_type_matrix() {
    assert_group(0, 359, 287);
}
#[test]
fn typed_bytes_and_input_origins() {
    assert_group(1, 282, 246);
}
#[test]
fn typed_parameter_inference_and_bounds() {
    assert_group(2, 136, 86);
}
#[test]
fn typed_schema_and_value_boundaries() {
    assert_group(3, 51, 38);
}
#[test]
fn typed_whole_validation_and_error_priority() {
    assert_group(4, 118, 31);
}

#[test]
fn generated_typed_predicate_operations() {
    let mut operations = 0;
    for seed in 0..64 {
        let cases = generated_cases(seed, 128);
        assert_eq!(cases.len(), 128);
        for case in cases {
            assert_case(&case);
            operations += 1;
        }
    }
    assert_eq!(operations, 8192);
    eprintln!(
        "typed generated seeds=64 operations=8192 compile_calls=8192 eval_calls=8192 full_arrays=true"
    );
}

#[test]
fn known_bad_typed_predicate_observations_are_rejected() {
    let mutations = known_bad_observations();
    assert_eq!(mutations.len(), 44);
    let mut controls = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for mutation in mutations {
        assert!(ids.insert(mutation.id), "duplicate mutation ID");
        if controls.insert(mutation.case.id.clone()) {
            assert!(check(&mutation.case, &mutation.case.expected));
        }
        assert_ne!(
            mutation.observed, mutation.case.expected,
            "{} no-op",
            mutation.id
        );
        assert!(
            !check(&mutation.case, &mutation.observed),
            "accepted {}",
            mutation.id
        );
    }
    assert_eq!(controls.len(), 8);
    eprintln!("typed known_bad=44 unchanged_controls=8 product_calls=0");
}

#[test]
#[ignore = "expanded valid typed grammar: 256 seeds × 1024 operations"]
fn extended_generated_typed_predicate_operations() {
    let mut operations = 0;
    for seed in 0..256 {
        let cases = generated_cases(seed, 1024);
        assert_eq!(cases.len(), 1024);
        for case in cases {
            assert_case(&case);
            operations += 1;
        }
    }
    assert_eq!(operations, 262144);
    eprintln!("typed expanded seeds=256 operations=262144 compile_calls=262144 eval_calls=262144");
}

#[test]
#[ignore = "separate 30-minute grammar-valid typed generator; no arbitrary malformed fuzz claim"]
fn extended_typed_predicate_generated_30_minutes() {
    let started = Instant::now();
    let mut seed = 256;
    let mut operations = 0;
    while started.elapsed() < Duration::from_secs(30 * 60) {
        for case in generated_cases(seed, 128) {
            assert_case(&case);
            operations += 1;
        }
        seed += 1;
    }
    assert!(operations >= 128);
    eprintln!(
        "typed grammar-valid timed seeds={} operations={operations} compile_calls={operations} eval_calls={operations}",
        seed - 256
    );
}
