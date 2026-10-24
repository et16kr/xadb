//! Independent scalar input data and exact observations for D4-FE-SCALAR-001.
//!
//! No production value, parser, conversion, normalization or validation helper
//! supplies expected results. Raw spelling recipes run from known values toward
//! input bytes; they never decode the input to calculate the expected value.

mod literals;
mod recipes;

pub use literals::{decode_literals, wrong_node_literals};
pub use recipes::{copy_boundaries, copy_matrix, decode_boundaries, generated_cases};

/// Original SQL logical type, including the type carried by NULL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeTag {
    /// Signed64 integer.
    Int64,
    /// Boolean.
    Bool,
    /// UTF-8 text measured in bytes.
    Utf8,
    /// Arbitrary binary bytes.
    Binary,
}

/// Exact expected payload; NULL is different from a zero-length byte vector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Payload {
    /// No value, with type retained separately.
    Null,
    /// Exact integer, without coercion or saturation.
    Int64(i64),
    /// Exact boolean.
    Bool(bool),
    /// Exact UTF-8 bytes, without trimming or normalization.
    Utf8(Vec<u8>),
    /// Exact arbitrary binary bytes.
    Binary(Vec<u8>),
}

/// A typed value independent of the production storage representation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueObservation {
    /// Original logical type, retained even for NULL.
    pub logical_type: TypeTag,
    /// Complete value payload.
    pub payload: Payload,
}

/// Pure input diagnostic with no statement or transaction effects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorObservation {
    /// Exact frontend error label.
    pub code: &'static str,
    /// Detecting module, normally Value in this leaf.
    pub module: &'static str,
    /// Inclusive start and exclusive end; copy calls preserve caller coordinates.
    pub span: [usize; 2],
}

/// One completed decoder/copy call result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// An owned typed value.
    Value(ValueObservation),
    /// Raw SQL NULL whose type still requires context.
    UntypedNull,
    /// Unresolved, one-based parameter ordinal.
    Parameter(u16),
    /// Terminal failure without a partially successful value.
    Error(ErrorObservation),
}

/// Observation completeness is checked independently of the result variant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScalarObservation {
    /// Whole returned result.
    pub outcome: Outcome,
    /// False denotes interruption, never successful completion.
    pub complete: bool,
}

/// Which input boundary owns empty-text semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// SQL literal/binding input normalizes zero-length UTF-8 only.
    Sql,
    /// Already-native input preserves non-NULL zero-length UTF-8.
    Native,
}

/// Explicit test metadata, without catalog or binding simulation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spec {
    /// Required logical type.
    pub logical_type: TypeTag,
    /// Variable maximum bytes; None for fixed Int64/Bool.
    pub maximum_bytes: Option<usize>,
    /// Whether a correctly typed NULL is permitted.
    pub nullable: bool,
}

/// An exact input call, with parser usage restricted to an immutable consumer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Call {
    /// Parse this fixed source, then consume the explicitly named scalar node.
    Decode {
        /// Exact SQL source for the existing raw parser.
        source: Vec<u8>,
        /// Fixed preorder array index, not derived from production output.
        node: usize,
    },
    /// Copy a typed caller input after explicit scalar metadata validation.
    Copy {
        /// SQL-input or native-preservation boundary.
        origin: Origin,
        /// Literal typed payload used to construct ScalarRef.
        input: ValueObservation,
        /// Explicit destination metadata.
        spec: Spec,
        /// Caller coordinates that every copy diagnostic must preserve unchanged.
        at: [usize; 2],
    },
}

/// One frozen input and independently supplied complete result.
#[derive(Clone, Debug)]
pub struct ScalarCase {
    /// Stable case identity.
    pub id: String,
    /// Exact operation and source data.
    pub call: Call,
    /// Independent expected outcome.
    pub expected: ScalarObservation,
}

/// Strict equality; no type erasure, byte conversion, or partial success.
pub fn check_scalar(case: &ScalarCase, observed: &ScalarObservation) -> bool {
    observed.complete && observed == &case.expected
}

pub(super) fn value(logical_type: TypeTag, payload: Payload) -> ValueObservation {
    ValueObservation {
        logical_type,
        payload,
    }
}

pub(super) fn outcome(logical_type: TypeTag, payload: Payload) -> Outcome {
    Outcome::Value(value(logical_type, payload))
}

pub(super) fn error(code: &'static str, span: [usize; 2]) -> Outcome {
    Outcome::Error(ErrorObservation {
        code,
        module: "Value",
        span,
    })
}

pub(super) fn case(id: impl Into<String>, call: Call, outcome: Outcome) -> ScalarCase {
    ScalarCase {
        id: id.into(),
        call,
        expected: ScalarObservation {
            outcome,
            complete: true,
        },
    }
}

pub(super) fn decode(id: impl Into<String>, raw: &[u8], expected: Outcome) -> ScalarCase {
    let mut source = b"INSERT INTO t(a) VALUES (".to_vec();
    source.extend_from_slice(raw);
    source.extend_from_slice(b");");
    case(id, Call::Decode { source, node: 7 }, expected)
}

pub(super) fn bad_decode(id: impl Into<String>, raw: &[u8], code: &'static str) -> ScalarCase {
    decode(id, raw, error(code, [25, 25 + raw.len()]))
}

pub(super) fn copy(
    id: impl Into<String>,
    origin: Origin,
    input: ValueObservation,
    spec: Spec,
    at: [usize; 2],
    expected: Outcome,
) -> ScalarCase {
    case(
        id,
        Call::Copy {
            origin,
            input,
            spec,
            at,
        },
        expected,
    )
}

/// One deliberately altered observation with its original exact acceptance case.
#[derive(Clone, Debug)]
pub struct KnownBad {
    /// Stable mutation label.
    pub id: &'static str,
    /// Original correct case.
    pub case: ScalarCase,
    /// Corrupted observation that must be rejected.
    pub observed: ScalarObservation,
}

/// Literal result mutations; no production function participates in this check.
pub fn known_bad_observations() -> Vec<KnownBad> {
    use TypeTag::{Binary, Int64, Utf8};
    let text = decode(
        "control-text",
        b"' x \0'",
        outcome(Utf8, Payload::Utf8(b" x \0".to_vec())),
    );
    let null = decode("control-sql-empty", b"''", outcome(Utf8, Payload::Null));
    let binary = decode(
        "control-empty-binary",
        b"X''",
        outcome(Binary, Payload::Binary(Vec::new())),
    );
    let native = copy(
        "control-native-empty",
        Origin::Native,
        value(Utf8, Payload::Utf8(Vec::new())),
        Spec {
            logical_type: Utf8,
            maximum_bytes: Some(1),
            nullable: false,
        },
        [91, 87],
        outcome(Utf8, Payload::Utf8(Vec::new())),
    );
    let minimum = decode(
        "control-minimum",
        b"-9223372036854775808",
        outcome(Int64, Payload::Int64(i64::MIN)),
    );
    let parameter = decode("control-parameter", b"?", Outcome::Parameter(1));
    let raw_null = decode("control-raw-null", b"NULL", Outcome::UntypedNull);
    let failed = bad_decode("control-hex-error", b"X'GG'", "Syntax");
    let mut mutations = Vec::new();
    let mut add = |id, case: &ScalarCase, observed| {
        mutations.push(KnownBad {
            id,
            case: case.clone(),
            observed,
        })
    };
    let mut observed = text.expected.clone();
    observed.complete = false;
    add("incomplete-success", &text, observed);
    let mut observed = null.expected.clone();
    observed.outcome = Outcome::UntypedNull;
    add("typed-null-erased", &null, observed);
    let mut observed = null.expected.clone();
    observed.outcome = outcome(Int64, Payload::Null);
    add("typed-null-retyped", &null, observed);
    let mut observed = null.expected.clone();
    observed.outcome = outcome(Utf8, Payload::Utf8(Vec::new()));
    add("sql-empty-not-normalized", &null, observed);
    let mut observed = native.expected.clone();
    observed.outcome = outcome(Utf8, Payload::Null);
    add("native-empty-normalized", &native, observed);
    let mut observed = binary.expected.clone();
    observed.outcome = outcome(Binary, Payload::Null);
    add("empty-binary-normalized", &binary, observed);
    let mut observed = text.expected.clone();
    observed.outcome = outcome(Utf8, Payload::Utf8(b" x ".to_vec()));
    add("nul-truncated", &text, observed);
    let mut observed = text.expected.clone();
    observed.outcome = outcome(Utf8, Payload::Utf8(b"x \0".to_vec()));
    add("space-trimmed", &text, observed);
    let mut observed = text.expected.clone();
    observed.outcome = outcome(Binary, Payload::Binary(b" x \0".to_vec()));
    add("text-binary-coercion", &text, observed);
    let mut observed = text.expected.clone();
    observed.outcome = outcome(Utf8, Payload::Binary(b" x \0".to_vec()));
    add("inconsistent-payload-tag", &text, observed);
    let mut observed = minimum.expected.clone();
    observed.outcome = outcome(Int64, Payload::Int64(i64::MAX));
    add("minimum-wrapped", &minimum, observed);
    let mut observed = minimum.expected.clone();
    observed.outcome = error("Range", [25, 45]);
    add("minimum-rejected", &minimum, observed);
    let mut observed = parameter.expected.clone();
    observed.outcome = Outcome::Parameter(0);
    add("parameter-zero-based", &parameter, observed);
    let mut observed = parameter.expected.clone();
    observed.outcome = outcome(Int64, Payload::Int64(0));
    add("parameter-fabricated-value", &parameter, observed);
    let mut observed = raw_null.expected.clone();
    observed.outcome = outcome(Utf8, Payload::Null);
    add("raw-null-premature-type", &raw_null, observed);
    let mut observed = failed.expected.clone();
    observed.outcome = outcome(Binary, Payload::Binary(Vec::new()));
    add("malformed-hex-success", &failed, observed);
    let mut observed = failed.expected.clone();
    observed.outcome = error("Range", [25, 30]);
    add("wrong-error-code", &failed, observed);
    let mut observed = failed.expected.clone();
    if let Outcome::Error(value) = &mut observed.outcome {
        value.module = "Parse";
    }
    add("wrong-error-module", &failed, observed);
    let mut observed = failed.expected.clone();
    observed.outcome = error("Syntax", [25, 25]);
    add("wrong-error-span", &failed, observed);
    let mut observed = failed.expected.clone();
    observed.complete = false;
    add("incomplete-error", &failed, observed);
    mutations
}
