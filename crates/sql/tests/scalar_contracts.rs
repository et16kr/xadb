use foundation::runtime::{Cancellation, Clock, Quota};
use sql::frontend::{
    CallControl, Code, DeclaredType, DecodedScalar, Diagnostic, Module, Parser, ScalarReader,
    ScalarRef, ScalarSpec, ScalarType, Span,
};
use std::collections::BTreeSet;
use std::time::{Duration, Instant};
use test_reference::frontend::scalar::{
    Call, ErrorObservation, Origin, Outcome, Payload, ScalarCase, ScalarObservation, Spec, TypeTag,
    ValueObservation, check_scalar, copy_boundaries, copy_matrix, decode_boundaries,
    decode_literals, generated_cases, known_bad_observations, wrong_node_literals,
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

fn input_ref(value: &ValueObservation) -> ScalarRef<'_> {
    match &value.payload {
        Payload::Null => ScalarRef::Null(scalar_type(value.logical_type)),
        Payload::Int64(number) => {
            assert_eq!(value.logical_type, TypeTag::Int64, "fixture input tag");
            ScalarRef::Int64(*number)
        }
        Payload::Bool(boolean) => {
            assert_eq!(value.logical_type, TypeTag::Bool, "fixture input tag");
            ScalarRef::Bool(*boolean)
        }
        Payload::Utf8(bytes) => {
            assert_eq!(value.logical_type, TypeTag::Utf8, "fixture input tag");
            ScalarRef::Utf8(std::str::from_utf8(bytes).expect("literal UTF-8 fixture input"))
        }
        Payload::Binary(bytes) => {
            assert_eq!(value.logical_type, TypeTag::Binary, "fixture input tag");
            ScalarRef::Binary(bytes)
        }
    }
}

fn input_spec(spec: Spec) -> ScalarSpec {
    let declared_type = match spec.logical_type {
        TypeTag::Int64 => {
            assert_eq!(spec.maximum_bytes, None, "fixed input metadata");
            DeclaredType::Int64
        }
        TypeTag::Bool => {
            assert_eq!(spec.maximum_bytes, None, "fixed input metadata");
            DeclaredType::Bool
        }
        TypeTag::Utf8 => DeclaredType::Utf8(spec.maximum_bytes.expect("literal variable metadata")),
        TypeTag::Binary => {
            DeclaredType::Binary(spec.maximum_bytes.expect("literal variable metadata"))
        }
    };
    ScalarSpec {
        declared_type,
        nullable: spec.nullable,
    }
}

fn value_observation(value: ScalarRef<'_>) -> ValueObservation {
    let (logical_type, payload) = match value {
        ScalarRef::Null(kind) => (observed_type(kind), Payload::Null),
        ScalarRef::Int64(number) => (TypeTag::Int64, Payload::Int64(number)),
        ScalarRef::Bool(boolean) => (TypeTag::Bool, Payload::Bool(boolean)),
        ScalarRef::Utf8(text) => (TypeTag::Utf8, Payload::Utf8(text.as_bytes().to_vec())),
        ScalarRef::Binary(bytes) => (TypeTag::Binary, Payload::Binary(bytes.to_vec())),
    };
    ValueObservation {
        logical_type,
        payload,
    }
}

fn error_observation(error: Diagnostic) -> Outcome {
    Outcome::Error(ErrorObservation {
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
    })
}

fn assert_case(case: &ScalarCase) {
    let session = Quota::new(8 * 1024 * 1024);
    let database = Quota::new(64 * 1024 * 1024);
    let cancellation = Cancellation::default();
    let control = CallControl::new(&cancellation, &FixedClock, None);
    let mut reader = ScalarReader::new(&session, &database);
    let outcome = match &case.call {
        Call::Decode { source, node } => {
            let mut parser = Parser::new(&session, &database);
            let statement = parser.parse(source, &control).unwrap_or_else(|error| {
                panic!(
                    "{} fixed scalar source failed raw parsing: {error:?}; source length={}",
                    case.id,
                    source.len(),
                )
            });
            match reader.decode(&statement, *node, &control) {
                Ok(DecodedScalar::Value(value)) => {
                    Outcome::Value(value_observation(value.as_ref()))
                }
                Ok(DecodedScalar::Null) => Outcome::UntypedNull,
                Ok(DecodedScalar::Parameter(ordinal)) => Outcome::Parameter(ordinal),
                Err(error) => error_observation(error),
            }
        }
        Call::Copy {
            origin,
            input,
            spec,
            at,
        } => {
            let input = input_ref(input);
            let spec = input_spec(*spec);
            let at = Span {
                start: at[0],
                end: at[1],
            };
            let result = match origin {
                Origin::Sql => reader.copy_sql(input, spec, at, &control),
                Origin::Native => reader.copy_native(input, spec, at, &control),
            };
            match result {
                Ok(value) => Outcome::Value(value_observation(value.as_ref())),
                Err(error) => error_observation(error),
            }
        }
    };
    let observed = ScalarObservation {
        outcome,
        complete: true,
    };
    assert!(
        check_scalar(case, &observed),
        "case={} observed={} expected={} complete={}/{}",
        case.id,
        outcome_summary(&observed.outcome),
        outcome_summary(&case.expected.outcome),
        observed.complete,
        case.expected.complete,
    );
    drop(reader);
    assert_eq!(session.used(), 0, "{} session cleanup", case.id);
    assert_eq!(database.used(), 0, "{} database cleanup", case.id);
}

fn outcome_summary(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Value(value) => {
            let payload = match &value.payload {
                Payload::Utf8(bytes) | Payload::Binary(bytes) => format!(
                    "bytes={} prefix={:?} suffix={:?}",
                    bytes.len(),
                    &bytes[..bytes.len().min(16)],
                    &bytes[bytes.len().saturating_sub(16)..],
                ),
                other => format!("{other:?}"),
            };
            format!("{:?} {payload}", value.logical_type)
        }
        other => format!("{other:?}"),
    }
}

#[test]
fn literal_scalar_decode() {
    let cases = decode_literals();
    assert_eq!(cases.len(), 401);
    for case in cases {
        assert_case(&case);
    }
}

#[test]
fn scalar_node_consumer_boundaries() {
    let cases = wrong_node_literals();
    assert_eq!(cases.len(), 10);
    for case in cases {
        assert_case(&case);
    }
}

#[test]
fn decoded_scalar_byte_and_range_boundaries() {
    let cases = decode_boundaries();
    assert_eq!(cases.len(), 36);
    for case in cases {
        assert_case(&case);
    }
}

// This exact detector rejects both the typed-NULL mismatch acceptance mutant
// and the native-empty-to-NULL mutant using separate rows of the frozen table.
#[test]
fn typed_input_type_null_origin_matrix() {
    let cases = copy_matrix();
    assert_eq!(cases.len(), 192);
    for case in cases {
        assert_case(&case);
    }
}

#[test]
fn typed_input_metadata_and_byte_boundaries() {
    let cases = copy_boundaries();
    assert_eq!(cases.len(), 98);
    for case in cases {
        assert_case(&case);
    }
}

#[test]
fn generated_scalar_input_operations() {
    let mut calls = 0;
    for seed in 0..64 {
        let cases = generated_cases(seed, 128);
        assert_eq!(cases.len(), 128);
        for case in cases {
            assert_case(&case);
            calls += 1;
        }
    }
    assert_eq!(calls, 8192);
}

#[test]
fn known_bad_scalar_observations_are_rejected() {
    let mutations = known_bad_observations();
    assert_eq!(mutations.len(), 20);
    let mut controls = BTreeSet::new();
    for mutation in mutations {
        if controls.insert(mutation.case.id.clone()) {
            assert!(
                check_scalar(&mutation.case, &mutation.case.expected),
                "{} rejected exact acceptance control",
                mutation.case.id,
            );
        }
        assert!(
            !check_scalar(&mutation.case, &mutation.observed),
            "{} accepted known-bad result for {}",
            mutation.id,
            mutation.case.id,
        );
    }
    assert_eq!(controls.len(), 8);
}

#[test]
#[ignore = "Separately selected scalar expansion: 256 seeds x 1024 operations"]
fn extended_generated_scalar_input_operations() {
    for seed in 0..256 {
        for case in generated_cases(seed, 1024) {
            assert_case(&case);
        }
    }
}

#[test]
#[ignore = "Separately selected 30-minute grammar-valid generated scalar profile, not arbitrary-byte fuzz"]
fn extended_scalar_generated_30_minutes() {
    let started = Instant::now();
    let mut seed = 256_u64;
    while started.elapsed() < Duration::from_secs(1800) {
        for case in generated_cases(seed, 128) {
            assert_case(&case);
        }
        seed = seed
            .checked_add(1)
            .expect("bounded generated seed sequence");
    }
    assert!(seed > 256, "generated profile completed no seed");
    println!(
        "Scalar grammar-valid generated profile: completed-seeds={} calls={} seeds=256..{} elapsed={:?}; arbitrary malformed bytes and exhaustive coverage are not claimed",
        seed - 256,
        (seed - 256) * 128,
        seed - 1,
        started.elapsed(),
    );
}
