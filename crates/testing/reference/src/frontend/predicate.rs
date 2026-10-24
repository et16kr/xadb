//! Independent predicate syntax observations, literal inputs and fixed recipes.
//!
//! Expected arrays are authored directly or assembled from known fragments. No
//! production lexer, parser, normalizer, decoder or evaluator computes an answer.

mod literals;
mod recipes;

use super::parser::{ErrorObservation, NameObservation, ScalarObservation};

pub use literals::{error_cases, literal_shapes, raw_value_cases};
pub use recipes::{boundary_cases, generated_case};

/// One of the six syntactic comparison operators, without value evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Comparison {
    /// Equality.
    Eq,
    /// Inequality written as `<>`.
    Ne,
    /// Strictly less than.
    Lt,
    /// Less than or equal.
    Le,
    /// Strictly greater than.
    Gt,
    /// Greater than or equal.
    Ge,
}

/// Complete public predicate-node payload in the independent observation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The single fragment root.
    Predicate,
    /// Ordered disjunction, including a singleton.
    Or,
    /// Ordered conjunction, including a singleton.
    And,
    /// Prefix NOT with one predicate child.
    Not,
    /// Parenthesized predicate with one Or child.
    Group,
    /// Comparison with two value children.
    Compare(Comparison),
    /// NULL test with one value child.
    IsNull {
        /// True only for IS NOT NULL.
        negated: bool,
    },
    /// Independently fixed normalized name and original metadata.
    Name(NameObservation),
    /// Unconverted scalar syntax.
    Scalar(ScalarObservation),
}

/// One entry of the complete preorder array, including otherwise unreachable data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    /// Exact node role and payload.
    pub kind: Kind,
    /// Half-open source-byte interval.
    pub span: [usize; 2],
    /// First direct child's array index.
    pub first_child: Option<usize>,
    /// Next direct sibling's array index.
    pub next_sibling: Option<usize>,
}

/// Complete parse outcome; a failure exposes no partial source or tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    /// All retained bytes, including outer trivia and the optional semicolon.
    pub source: Vec<u8>,
    /// Every public arena entry in its original order.
    pub nodes: Vec<Node>,
    /// Number of markers assigned in lexical occurrence order.
    pub marker_count: u16,
    /// Exact failure module, code and byte span.
    pub error: Option<ErrorObservation>,
    /// False is an incomplete observation and is always rejected.
    pub complete: bool,
}

/// One immutable input and its independently supplied complete expectation.
#[derive(Clone, Debug)]
pub struct Case {
    /// Unique stable fixture identity.
    pub id: String,
    /// Exact caller bytes.
    pub source: Vec<u8>,
    /// Independently authored outcome.
    pub expected: Observation,
}

/// Compare all outcome fields, array entries and payloads without graph pruning.
pub fn check(case: &Case, observed: &Observation) -> bool {
    observed.complete && case.expected == *observed
}

fn node(kind: Kind, span: [usize; 2], first: Option<usize>, next: Option<usize>) -> Node {
    Node {
        kind,
        span,
        first_child: first,
        next_sibling: next,
    }
}

fn name(bytes: &[u8], quoted: bool, span: [usize; 2], next: Option<usize>) -> Node {
    node(
        Kind::Name(NameObservation {
            bytes: bytes.to_vec(),
            quoted,
            span,
        }),
        span,
        None,
        next,
    )
}

fn good(id: impl Into<String>, source: &[u8], nodes: Vec<Node>, marker_count: u16) -> Case {
    Case {
        id: id.into(),
        source: source.to_vec(),
        expected: Observation {
            source: source.to_vec(),
            nodes,
            marker_count,
            error: None,
            complete: true,
        },
    }
}

fn bad(
    id: impl Into<String>,
    source: &[u8],
    code: &'static str,
    module: &'static str,
    span: [usize; 2],
) -> Case {
    Case {
        id: id.into(),
        source: source.to_vec(),
        expected: Observation {
            source: Vec::new(),
            nodes: Vec::new(),
            marker_count: 0,
            error: Some(ErrorObservation { code, module, span }),
            complete: true,
        },
    }
}

/// A single named corruption of one correct observation.
#[derive(Clone, Debug)]
pub struct KnownBad {
    /// Unique corruption identity, separate from the fixture identity.
    pub id: &'static str,
    /// Unchanged independent input and expected outcome.
    pub case: Case,
    /// Deliberately wrong observation to reject.
    pub observed: Observation,
}

/// Return independent field corruptions and their unchanged acceptance controls.
pub fn known_bad_observations() -> Vec<KnownBad> {
    literals::known_bad_observations()
}
