//! Independent P2 parser observations and fixtures, without production semantics.
//!
//! Literal fragments carry their own decoded name bytes and scalar tags. Recipes
//! only concatenate those fragments and relocate their explicitly known spans.
//! They do not tokenize, normalize names, parse SQL, or inspect product results.

mod literals;
mod recipes;

pub use literals::{error_literals, structural_literals};
pub use recipes::{boundary_cases, generated_case, scalar_literals};

/// An exact owned name observation, including its own raw-source coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameObservation {
    /// Literal normalized identity bytes.
    pub bytes: Vec<u8>,
    /// Whether the original identifier used double quotes.
    pub quoted: bool,
    /// Inclusive start and exclusive end in the owned source.
    pub span: [usize; 2],
}

/// Raw syntax only: no Int64, hex, empty-text, or decoded-length validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScalarObservation {
    /// Raw decimal digits and an optional separate sign.
    Integer {
        /// Exactly None, Plus or Minus.
        sign: &'static str,
        /// Digits alone, excluding sign and intervening trivia.
        digits: [usize; 2],
    },
    /// Quoted text, including an empty raw literal.
    Text,
    /// Raw binary token, before hexadecimal value validation.
    Binary,
    /// TRUE or FALSE syntax.
    Bool(bool),
    /// An explicit NULL token.
    Null,
    /// A marker numbered in source order.
    Parameter {
        /// One-based marker ordinal.
        ordinal: u16,
    },
}

/// Every public P2 node tag and its complete payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KindObservation {
    /// INSERT statement root.
    Insert,
    /// BEGIN statement root.
    Begin,
    /// COMMIT statement root.
    Commit,
    /// ROLLBACK statement root.
    Rollback,
    /// SET AUTOCOMMIT root with the requested flag.
    SetAutocommit(bool),
    /// Target name, retaining an explicit namespace prefix.
    Target,
    /// Parenthesized column-name list.
    Columns,
    /// VALUES and its tuple list.
    Rows,
    /// One parenthesized scalar list.
    Tuple,
    /// Normalized fixed name and its original spelling coordinates.
    Name(NameObservation),
    /// Raw scalar syntax and any associated metadata.
    Scalar(ScalarObservation),
}

/// One array entry; comparisons include entries unreachable from the root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeObservation {
    /// Complete public node variant and payload.
    pub kind: KindObservation,
    /// Inclusive start and exclusive end in the source.
    pub span: [usize; 2],
    /// Array index of the first direct child.
    pub first_child: Option<usize>,
    /// Array index of the next direct sibling.
    pub next_sibling: Option<usize>,
}

/// Exact diagnostic; retained lexer/name modules must not become Parse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorObservation {
    /// Exact diagnostic code label.
    pub code: &'static str,
    /// Module that detected the failure.
    pub module: &'static str,
    /// Exact half-open diagnostic coordinates.
    pub span: [usize; 2],
}

/// A complete call result. Failed calls expose no partial owned AST or source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseObservation {
    /// All retained source bytes; empty on failure.
    pub source: Vec<u8>,
    /// Entire public arena in its original order; empty on failure.
    pub nodes: Vec<NodeObservation>,
    /// Count of successfully numbered markers; zero on failure.
    pub marker_count: u16,
    /// Terminal call diagnostic, if any.
    pub error: Option<ErrorObservation>,
    /// False denotes an interrupted observation, never successful completion.
    pub complete: bool,
}

/// One exact source and expected complete call result.
#[derive(Clone, Debug)]
pub struct ParseCase {
    /// Stable fixture identity.
    pub id: String,
    /// Exact caller input.
    pub source: Vec<u8>,
    /// Independently fixed result for this input.
    pub expected: ParseObservation,
}

/// Strict equality, including source ownership bytes, all nodes and completion.
pub fn check_parse(case: &ParseCase, observed: &ParseObservation) -> bool {
    observed.complete && observed == &case.expected
}

fn node(
    kind: KindObservation,
    span: [usize; 2],
    first: Option<usize>,
    next: Option<usize>,
) -> NodeObservation {
    NodeObservation {
        kind,
        span,
        first_child: first,
        next_sibling: next,
    }
}

fn name(bytes: &[u8], quoted: bool, span: [usize; 2], next: Option<usize>) -> NodeObservation {
    node(
        KindObservation::Name(NameObservation {
            bytes: bytes.to_vec(),
            quoted,
            span,
        }),
        span,
        None,
        next,
    )
}

fn good(
    id: impl Into<String>,
    source: &[u8],
    nodes: Vec<NodeObservation>,
    marker_count: u16,
) -> ParseCase {
    ParseCase {
        id: id.into(),
        source: source.to_vec(),
        expected: ParseObservation {
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
) -> ParseCase {
    ParseCase {
        id: id.into(),
        source: source.to_vec(),
        expected: ParseObservation {
            source: Vec::new(),
            nodes: Vec::new(),
            marker_count: 0,
            error: Some(ErrorObservation { code, module, span }),
            complete: true,
        },
    }
}

/// A named mutation of a correct result that the checker must reject.
#[derive(Clone, Debug)]
pub struct KnownBad {
    /// Stable mutation identity.
    pub id: &'static str,
    /// Original correct case.
    pub case: ParseCase,
    /// Deliberately corrupted result that must be rejected.
    pub observed: ParseObservation,
}

/// Exact array mutations; no production parser is involved in these controls.
pub fn known_bad_observations() -> Vec<KnownBad> {
    let base = literals::basic_insert();
    let marker = recipes::marker_insert(2);
    let empty_text = recipes::one_scalar("known-bad-empty-text", recipes::text(b"''"));
    let mut cases = Vec::new();
    let mut add = |id, case: &ParseCase, observed| {
        cases.push(KnownBad {
            id,
            case: case.clone(),
            observed,
        })
    };
    let mut observed = base.expected.clone();
    observed.complete = false;
    add("incomplete", &base, observed);
    let mut observed = base.expected.clone();
    observed.source.pop();
    add("source-truncated", &base, observed);
    let mut observed = base.expected.clone();
    observed.nodes.push(observed.nodes[7].clone());
    add("orphan-extra-node", &base, observed);
    let mut observed = base.expected.clone();
    observed.nodes[7].next_sibling = Some(7);
    add("self-cycle", &base, observed);
    let mut observed = base.expected.clone();
    observed.nodes[6].first_child = Some(6);
    add("parent-cycle", &base, observed);
    let mut observed = base.expected.clone();
    observed.nodes[3].first_child = Some(2);
    add("duplicate-edge", &base, observed);
    let mut observed = base.expected.clone();
    observed.nodes[0].first_child = Some(8);
    add("out-of-range-edge", &base, observed);
    let mut observed = base.expected.clone();
    observed.nodes.swap(2, 4);
    add("array-order", &base, observed);
    let mut observed = base.expected.clone();
    observed.nodes[3].next_sibling = None;
    add("missing-rows-edge", &base, observed);
    let mut observed = base.expected.clone();
    observed.nodes[0].span[1] += 1;
    add("root-includes-semicolon", &base, observed);
    let mut observed = base.expected.clone();
    if let KindObservation::Name(value) = &mut observed.nodes[2].kind {
        value.bytes = b"t".to_vec();
    }
    add("wrong-normalized-name", &base, observed);
    let mut observed = base.expected.clone();
    if let KindObservation::Name(value) = &mut observed.nodes[2].kind {
        value.quoted = true;
    }
    add("wrong-quoted-flag", &base, observed);
    let mut observed = base.expected.clone();
    if let KindObservation::Name(value) = &mut observed.nodes[2].kind {
        value.span[0] += 1;
    }
    add("name-payload-span", &base, observed);
    let mut observed = base.expected.clone();
    if let KindObservation::Scalar(ScalarObservation::Integer { digits, .. }) =
        &mut observed.nodes[7].kind
    {
        digits[0] += 1;
    }
    add("integer-digits-span", &base, observed);
    let mut observed = base.expected.clone();
    if let KindObservation::Scalar(ScalarObservation::Integer { sign, .. }) =
        &mut observed.nodes[7].kind
    {
        *sign = "Plus";
    }
    add("integer-sign", &base, observed);
    let mut observed = empty_text.expected.clone();
    observed.nodes[7].kind = KindObservation::Scalar(ScalarObservation::Null);
    add("empty-text-to-null", &empty_text, observed);
    let mut observed = marker.expected.clone();
    observed.nodes[7].kind = KindObservation::Scalar(ScalarObservation::Parameter { ordinal: 0 });
    add("marker-zero", &marker, observed);
    let mut observed = marker.expected.clone();
    observed.nodes[7].kind = KindObservation::Scalar(ScalarObservation::Parameter { ordinal: 2 });
    observed.nodes[9].kind = KindObservation::Scalar(ScalarObservation::Parameter { ordinal: 1 });
    add("marker-order", &marker, observed);
    let mut observed = marker.expected.clone();
    observed.marker_count = 1;
    add("marker-count", &marker, observed);
    let error_case = bad("known-bad-error", b"BEGIN x", "Syntax", "Parse", [6, 7]);
    let mut observed = error_case.expected.clone();
    observed.error = None;
    add("missing-error", &error_case, observed);
    let mut observed = error_case.expected.clone();
    observed.error.as_mut().expect("literal error").module = "Lex";
    add("wrong-error-module", &error_case, observed);
    let mut observed = error_case.expected.clone();
    observed.error.as_mut().expect("literal error").span = [6, 6];
    add("wrong-error-span", &error_case, observed);
    let mut observed = error_case.expected.clone();
    observed.nodes = base.expected.nodes.clone();
    add("partial-ast-on-error", &error_case, observed);
    cases
}
