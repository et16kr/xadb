//! Independent complete observations for typed predicate compilation/evaluation.
//!
//! Fixed arrays are authored before implementation. Truth and byte relations
//! come from literal tables; no production parser, value or evaluator is used.

mod literals;
mod recipes;

pub use super::predicate::Comparison;
pub use super::scalar::{ErrorObservation, Payload, Spec, TypeTag, ValueObservation};
pub use recipes::generated_cases;

/// Three-valued predicate result, distinct from scalar Bool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Truth {
    /// True, and the only WHERE-admitted value.
    T,
    /// False.
    F,
    /// SQL unknown.
    U,
}

/// Result category of one retained typed node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expression {
    /// A predicate result.
    Truth,
    /// One concrete scalar logical type.
    Scalar(TypeTag),
    /// Raw SQL NULL without an invented concrete type.
    UntypedNull,
}

/// Complete public typed-node role.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Predicate root.
    Predicate,
    /// Disjunction, including singleton wrappers.
    Or,
    /// Conjunction, including singleton wrappers.
    And,
    /// Logical negation.
    Not,
    /// Explicit parentheses.
    Group,
    /// Typed comparison.
    Compare {
        /// Comparison relation.
        op: Comparison,
        /// Concrete operands' common type, absent only for two raw NULLs.
        scalar_type: Option<TypeTag>,
    },
    /// NULL test, always producing true or false.
    IsNull {
        /// Whether this is IS NOT NULL.
        negated: bool,
    },
    /// Resolved row column.
    Column {
        /// Zero-based index in the supplied schema.
        ordinal: u8,
    },
    /// Complete decoded typed payload, including typed NULL.
    Literal(ValueObservation),
    /// Raw, untyped SQL NULL.
    Null,
    /// One distinct marker occurrence.
    Parameter {
        /// One-based ordinal in source order.
        ordinal: u16,
    },
}

/// One exact preorder array entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    /// Node role and complete role-specific fields.
    pub kind: Kind,
    /// Truth/scalar/untyped category.
    pub result_type: Expression,
    /// Original half-open byte coordinates.
    pub span: [usize; 2],
    /// First child array index.
    pub first_child: Option<usize>,
    /// Next sibling array index.
    pub next_sibling: Option<usize>,
}

/// Owned schema observation; input Name objects are not retained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Column {
    /// Complete canonical name bytes.
    pub name: Vec<u8>,
    /// Original declaration quoting flag.
    pub quoted: bool,
    /// Declared type, nullable and byte maximum.
    pub spec: Spec,
    /// First source use, absent for unused columns.
    pub first_use: Option<[usize; 2]>,
}

/// Complete inferred parameter metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parameter {
    /// One-based occurrence ordinal.
    pub ordinal: u16,
    /// Predicate-only inferred specification.
    pub spec: Spec,
    /// Original marker span.
    pub span: [usize; 2],
}

/// Whole successful compiled output, with no partial-prefix equivalence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// Whole predicate span excluding the final semicolon.
    pub span: [usize; 2],
    /// Every typed preorder entry.
    pub nodes: Vec<Node>,
    /// Every schema column, including unused ones.
    pub columns: Vec<Column>,
    /// Every marker in ordinal order.
    pub parameters: Vec<Parameter>,
}

/// Complete two-phase observation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    /// Whole plan on compile success; absent on failure.
    pub plan: Option<Plan>,
    /// Exact compile diagnostic, including original Value errors.
    pub compile_error: Option<ErrorObservation>,
    /// Whether evaluate was actually called.
    pub evaluation_attempted: bool,
    /// Truth only on successful evaluation.
    pub truth: Option<Truth>,
    /// WHERE selection only on successful evaluation.
    pub where_keep: Option<bool>,
    /// Exact evaluation input/control diagnostic.
    pub eval_error: Option<ErrorObservation>,
    /// False is interruption, never success.
    pub complete: bool,
}

/// Explicit declaration input, without a catalog substitute.
#[derive(Clone, Debug)]
pub struct ColumnInput {
    /// Exact spelling used to obtain a validated production Name.
    pub raw_name: Vec<u8>,
    /// Independent declared specification.
    pub spec: Spec,
}

/// One canonical input and its full literal observation.
#[derive(Clone, Debug)]
pub struct Case {
    /// Stable exact-input identity.
    pub id: String,
    /// Inventory family owning this canonical call.
    pub family: &'static str,
    /// Exact raw predicate source.
    pub source: Vec<u8>,
    /// Ordered schema declaration.
    pub columns: Vec<ColumnInput>,
    /// Omitted hints or an exact caller-supplied array.
    pub hints: Option<Vec<Option<TypeTag>>>,
    /// Borrowed native row values, preserving native empty text.
    pub row: Vec<ValueObservation>,
    /// Borrowed SQL parameters, with original typed tags.
    pub parameters: Vec<ValueObservation>,
    /// Whether this case requests evaluation after compilation.
    pub evaluate: bool,
    /// Whole independently supplied result.
    pub expected: Observation,
}

/// Fixed canonical cases; coverage aliases are recorded in the JSON manifest.
pub fn literal_cases() -> Vec<Case> {
    literals::cases()
}

/// Strict complete equality without byte/type erasure or prefix acceptance.
pub fn check(case: &Case, actual: &Observation) -> bool {
    actual.complete && actual == &case.expected
}

fn value(logical_type: TypeTag, payload: Payload) -> ValueObservation {
    ValueObservation {
        logical_type,
        payload,
    }
}

fn spec(logical_type: TypeTag, maximum_bytes: Option<usize>, nullable: bool) -> Spec {
    Spec {
        logical_type,
        maximum_bytes,
        nullable,
    }
}

fn node(
    kind: Kind,
    result_type: Expression,
    span: [usize; 2],
    first_child: Option<usize>,
    next_sibling: Option<usize>,
) -> Node {
    Node {
        kind,
        result_type,
        span,
        first_child,
        next_sibling,
    }
}

/// One deliberately wrong complete observation and its unchanged control.
#[derive(Clone, Debug)]
pub struct KnownBad {
    /// Stable mutation identity.
    pub id: &'static str,
    /// Original fixed case.
    pub case: Case,
    /// Deliberately changed output.
    pub observed: Observation,
}

/// Wrong observations proving exact checking of truth, metadata and diagnostics.
pub fn known_bad_observations() -> Vec<KnownBad> {
    let cases = literal_cases();
    let select = |id: &str| {
        cases
            .iter()
            .find(|case| case.id == id)
            .expect("frozen known-bad control")
            .clone()
    };
    let mut out = Vec::new();
    let mut add = |id: &'static str, case: Case, mutate: fn(&mut Observation)| {
        let mut observed = case.expected.clone();
        mutate(&mut observed);
        assert_ne!(observed, case.expected, "known-bad must change output");
        out.push(KnownBad { id, case, observed });
    };
    let unknown = cases
        .iter()
        .find(|case| case.expected.truth == Some(Truth::U))
        .expect("unknown control")
        .clone();
    let true_case = cases
        .iter()
        .find(|case| case.expected.truth == Some(Truth::T))
        .expect("true control")
        .clone();
    let column = cases
        .iter()
        .find(|case| {
            case.expected
                .plan
                .as_ref()
                .is_some_and(|p| !p.columns.is_empty() && p.columns[0].first_use.is_some())
        })
        .expect("column control")
        .clone();
    let parameter = cases
        .iter()
        .find(|case| {
            case.expected.plan.as_ref().is_some_and(|p| {
                p.parameters
                    .iter()
                    .any(|m| m.spec.logical_type == TypeTag::Utf8)
            })
        })
        .expect("parameter control")
        .clone();
    let null = cases.iter().find(|case| case.expected.plan.as_ref().is_some_and(|p| p.nodes.iter().any(|n| matches!(&n.kind, Kind::Literal(v) if v.logical_type==TypeTag::Utf8 && v.payload==Payload::Null)))).expect("typed null control").clone();
    let binary = cases.iter().find(|case| case.expected.plan.as_ref().is_some_and(|p| p.nodes.iter().any(|n| matches!(&n.kind, Kind::Literal(v) if matches!(&v.payload, Payload::Binary(bytes) if !bytes.is_empty()))))).expect("binary control").clone();
    let compile_error = select("leading_trivia/07-phase_precedence-type-before-unknown");
    let eval_error = select("leading_trivia/03-input_priority-row-tag-before-param-tag");
    add("truth-unknown-as-false", unknown.clone(), |o| {
        o.truth = Some(Truth::F)
    });
    add("truth-unknown-as-true", unknown.clone(), |o| {
        o.truth = Some(Truth::T)
    });
    add("where-accept-unknown", unknown.clone(), |o| {
        o.where_keep = Some(true)
    });
    add("where-drop-true", true_case.clone(), |o| {
        o.where_keep = Some(false)
    });
    add("incomplete", true_case.clone(), |o| o.complete = false);
    add("missing-plan", true_case.clone(), |o| o.plan = None);
    add("partial-nodes", true_case.clone(), |o| {
        o.plan.as_mut().unwrap().nodes.pop();
    });
    add("extra-node", true_case.clone(), |o| {
        let p = o.plan.as_mut().unwrap();
        p.nodes.push(p.nodes[0].clone());
    });
    add("root-span", true_case.clone(), |o| {
        o.plan.as_mut().unwrap().span[0] += 1
    });
    add("node-span", true_case.clone(), |o| {
        o.plan.as_mut().unwrap().nodes[0].span[1] += 1
    });
    add("first-child", true_case.clone(), |o| {
        o.plan.as_mut().unwrap().nodes[0].first_child = None
    });
    add("next-sibling", true_case.clone(), |o| {
        o.plan.as_mut().unwrap().nodes[0].next_sibling = Some(0)
    });
    add("truth-as-scalar-bool", true_case.clone(), |o| {
        o.plan.as_mut().unwrap().nodes[0].result_type = Expression::Scalar(TypeTag::Bool)
    });
    add("column-name", column.clone(), |o| {
        o.plan.as_mut().unwrap().columns[0].name.push(b'X')
    });
    add("column-quoted", column.clone(), |o| {
        let p = &mut o.plan.as_mut().unwrap().columns[0];
        p.quoted = !p.quoted;
    });
    add("column-nullable", column.clone(), |o| {
        let p = &mut o.plan.as_mut().unwrap().columns[0];
        p.spec.nullable = !p.spec.nullable;
    });
    add("column-first-use", column.clone(), |o| {
        o.plan.as_mut().unwrap().columns[0].first_use = None
    });
    add("column-ordinal", column.clone(), |o| {
        let n = o
            .plan
            .as_mut()
            .unwrap()
            .nodes
            .iter_mut()
            .find(|n| matches!(n.kind, Kind::Column { .. }))
            .unwrap();
        if let Kind::Column { ordinal } = &mut n.kind {
            *ordinal += 1;
        }
    });
    add("missing-column", column.clone(), |o| {
        o.plan.as_mut().unwrap().columns.pop();
    });
    add("parameter-ordinal", parameter.clone(), |o| {
        o.plan.as_mut().unwrap().parameters[0].ordinal += 1
    });
    add("parameter-type", parameter.clone(), |o| {
        o.plan.as_mut().unwrap().parameters[0].spec.logical_type = TypeTag::Bool
    });
    add("parameter-nullable", parameter.clone(), |o| {
        o.plan.as_mut().unwrap().parameters[0].spec.nullable = false
    });
    add("parameter-length", parameter.clone(), |o| {
        o.plan.as_mut().unwrap().parameters[0].spec.maximum_bytes = Some(1)
    });
    add("parameter-span", parameter.clone(), |o| {
        o.plan.as_mut().unwrap().parameters[0].span[0] += 1
    });
    add("missing-parameter", parameter.clone(), |o| {
        o.plan.as_mut().unwrap().parameters.pop();
    });
    add("typed-null-type", null.clone(), |o| {
        let n = o
            .plan
            .as_mut()
            .unwrap()
            .nodes
            .iter_mut()
            .find(|n| matches!(&n.kind, Kind::Literal(v) if v.payload==Payload::Null))
            .unwrap();
        if let Kind::Literal(v) = &mut n.kind {
            v.logical_type = TypeTag::Binary;
        }
    });
    add("sql-empty-nonnull", null.clone(), |o| {
        let n = o
            .plan
            .as_mut()
            .unwrap()
            .nodes
            .iter_mut()
            .find(|n| matches!(&n.kind, Kind::Literal(v) if v.payload==Payload::Null))
            .unwrap();
        if let Kind::Literal(v) = &mut n.kind {
            v.payload = Payload::Utf8(Vec::new());
        }
    });
    add("literal-byte", binary.clone(), |o| {
        let n=o.plan.as_mut().unwrap().nodes.iter_mut().find(|n| matches!(&n.kind, Kind::Literal(v) if matches!(&v.payload, Payload::Binary(b) if !b.is_empty()))).unwrap();
        if let Kind::Literal(v) = &mut n.kind
            && let Payload::Binary(b) = &mut v.payload
        {
            b[0] ^= 1;
        }
    });
    add("compile-error-code", compile_error.clone(), |o| {
        o.compile_error.as_mut().unwrap().code = "Name"
    });
    add("compile-error-module", compile_error.clone(), |o| {
        o.compile_error.as_mut().unwrap().module = "Value"
    });
    add("compile-error-span", compile_error.clone(), |o| {
        o.compile_error.as_mut().unwrap().span = [0, 0]
    });
    add("evaluate-after-compile-error", compile_error.clone(), |o| {
        o.evaluation_attempted = true
    });
    add("eval-error-code", eval_error.clone(), |o| {
        o.eval_error.as_mut().unwrap().code = "NotNull"
    });
    add("eval-error-module", eval_error.clone(), |o| {
        o.eval_error.as_mut().unwrap().module = "Compile"
    });
    add("eval-error-span", eval_error.clone(), |o| {
        o.eval_error.as_mut().unwrap().span = [0, 0]
    });
    add("truth-on-eval-error", eval_error, |o| {
        o.truth = Some(Truth::F)
    });
    add("comparison-operator", true_case.clone(), |o| {
        let n = o
            .plan
            .as_mut()
            .unwrap()
            .nodes
            .iter_mut()
            .find(|n| matches!(n.kind, Kind::Compare { .. }))
            .unwrap();
        if let Kind::Compare { op, .. } = &mut n.kind {
            *op = Comparison::Ne;
        }
    });
    add("raw-null-invented-comparison-type", unknown.clone(), |o| {
        let n = o
            .plan
            .as_mut()
            .unwrap()
            .nodes
            .iter_mut()
            .find(|n| {
                matches!(
                    n.kind,
                    Kind::Compare {
                        scalar_type: None,
                        ..
                    }
                )
            })
            .unwrap();
        if let Kind::Compare { scalar_type, .. } = &mut n.kind {
            *scalar_type = Some(TypeTag::Int64);
        }
    });
    add("raw-null-invented-node-type", unknown.clone(), |o| {
        let n = o
            .plan
            .as_mut()
            .unwrap()
            .nodes
            .iter_mut()
            .find(|n| n.kind == Kind::Null)
            .unwrap();
        n.result_type = Expression::Scalar(TypeTag::Int64);
    });
    add("raw-null-invented-literal", unknown, |o| {
        let n = o
            .plan
            .as_mut()
            .unwrap()
            .nodes
            .iter_mut()
            .find(|n| n.kind == Kind::Null)
            .unwrap();
        n.kind = Kind::Literal(value(TypeTag::Int64, Payload::Null));
    });
    add("parameter-node-ordinal", parameter, |o| {
        let n = o
            .plan
            .as_mut()
            .unwrap()
            .nodes
            .iter_mut()
            .find(|n| matches!(n.kind, Kind::Parameter { .. }))
            .unwrap();
        if let Kind::Parameter { ordinal } = &mut n.kind {
            *ordinal += 1;
        }
    });
    add("column-declared-length", column, |o| {
        o.plan.as_mut().unwrap().columns[0].spec.maximum_bytes = Some(1)
    });
    add("missing-evaluation", true_case.clone(), |o| {
        o.evaluation_attempted = false
    });
    add("missing-truth", true_case, |o| o.truth = None);
    out
}
