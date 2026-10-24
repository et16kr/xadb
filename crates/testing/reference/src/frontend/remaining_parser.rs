//! Independent raw SELECT/DML/DDL observations, with no production parser calls.
//!
//! Source fragments and complete expected trees are authored before the product.
//! Equality includes every arena entry, including unreachable or extra entries.

mod literals;
mod recipes;

pub use super::parser::{ErrorObservation, NameObservation, ScalarObservation};
pub use super::predicate::Comparison;
pub use literals::{
    changed_dispatch_cases, contextual_cases, diagnostic_cases, raw_payload_cases, structural_cases,
};
pub use recipes::{boundary_cases, generated_cases};

/// An explicit ordering direction; absence remains separate in the node payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// ASC was present.
    Asc,
    /// DESC was present.
    Desc,
}

/// An explicit NULL placement, without planner defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NullPlacement {
    /// NULLS FIRST was present.
    First,
    /// NULLS LAST was present.
    Last,
}

/// The original column declaration, without primary-key interpretation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nullability {
    /// No nullability clause was supplied.
    Implicit,
    /// NULL was explicitly supplied.
    Null,
    /// NOT NULL was explicitly supplied.
    NotNull,
}

/// Raw type-length spelling; no range conversion is part of this observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypeLength {
    /// Exactly None, Plus, or Minus.
    pub sign: &'static str,
    /// The decimal digit token alone.
    pub digits: [usize; 2],
    /// Sign through final digit, including any intervening trivia.
    pub span: [usize; 2],
}

/// Original supported type syntax, including unvalidated variable lengths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataType {
    /// BIGINT.
    Int64,
    /// BOOLEAN.
    Bool,
    /// VARCHAR and its raw length.
    Utf8(TypeLength),
    /// VARBINARY and its raw length.
    Binary(TypeLength),
}

/// Complete node tags and payloads for the remaining statement grammar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// SELECT root.
    Select,
    /// UPDATE root.
    Update,
    /// DELETE root.
    Delete,
    /// CREATE TABLE root.
    CreateTable,
    /// DROP TABLE root.
    DropTable,
    /// CREATE INDEX root with its explicit UNIQUE modifier.
    CreateIndex {
        /// Whether UNIQUE was present.
        unique: bool,
    },
    /// DROP INDEX root.
    DropIndex,
    /// Projection list, excluding SELECT and FROM.
    Projection,
    /// One input name and optional explicit AS alias.
    ProjectionItem,
    /// A sole projection star.
    Wildcard,
    /// SET and all assignments.
    Assignments,
    /// One target name and raw scalar.
    Assignment,
    /// WHERE and an Or subtree, without a Predicate root.
    Where,
    /// ORDER BY and all terms.
    OrderBy,
    /// One input column and its optional explicit ordering metadata.
    OrderTerm {
        /// Explicit ASC/DESC; None is not replaced by ASC.
        direction: Option<Direction>,
        /// Explicit NULLS placement; None remains unspecified.
        nulls: Option<NullPlacement>,
    },
    /// LIMIT and one raw unsigned integer or marker.
    Limit,
    /// Parenthesized table definitions and table constraints.
    TableDefinition,
    /// One column declaration with original nullability.
    ColumnDefinition {
        /// Original absent, NULL, or NOT NULL clause.
        nullability: Nullability,
    },
    /// Supported type name and any raw length.
    DataType(DataType),
    /// A table-level PRIMARY KEY.
    PrimaryKey,
    /// A table-level UNIQUE constraint.
    UniqueConstraint,
    /// Mandatory STORAGE DISK.
    StorageDisk,
    /// Target name, retaining an optional PUBLIC prefix.
    Target,
    /// Parenthesized key-column names.
    Columns,
    /// Ordered disjunction, including a singleton.
    Or,
    /// Ordered conjunction, including a singleton.
    And,
    /// Prefix NOT.
    Not,
    /// Parenthesized predicate, with an Or child.
    Group,
    /// Raw comparison with two operands.
    Compare(Comparison),
    /// IS NULL or IS NOT NULL with one operand.
    IsNull {
        /// True for IS NOT NULL.
        negated: bool,
    },
    /// Independently authored normalized name and original coordinates.
    Name(NameObservation),
    /// Unconverted scalar syntax.
    Scalar(ScalarObservation),
    /// An observed node outside this grammar, which cannot equal a fixture node.
    Unexpected(String),
}

/// One element of the complete flat preorder arena.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    /// Original kind and complete payload.
    pub kind: Kind,
    /// Exact half-open source byte interval.
    pub span: [usize; 2],
    /// First direct child in the array.
    pub first_child: Option<usize>,
    /// Next direct sibling in the array.
    pub next_sibling: Option<usize>,
}

/// A complete parse call; errors expose no partial source, arena, or markers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    /// All retained caller bytes, including outer trivia and a final semicolon.
    pub source: Vec<u8>,
    /// Every arena entry in original preorder.
    pub nodes: Vec<Node>,
    /// Total one-based markers assigned across all clauses.
    pub marker_count: u16,
    /// Exact terminal diagnostic.
    pub error: Option<ErrorObservation>,
    /// False is always rejected, including an otherwise empty observation.
    pub complete: bool,
}

/// Independently authored source and full expected observation.
#[derive(Clone, Debug)]
pub struct Case {
    /// Stable, unique input identity.
    pub id: String,
    /// One of structural, raw, contextual, diagnostic, boundary, dispatch, generated.
    pub family: &'static str,
    /// Exact caller bytes.
    pub source: Vec<u8>,
    /// Complete literal or fragment-assembled expectation.
    pub expected: Observation,
}

/// Exact comparison of all fields; the checker never follows only reachable edges.
pub fn check(case: &Case, observed: &Observation) -> bool {
    observed.complete && observed == &case.expected
}

/// One deliberately damaged observation and its unchanged independent control.
#[derive(Clone, Debug)]
pub struct KnownBad {
    /// Stable mutation identity.
    pub id: &'static str,
    /// Original exact source and control observation.
    pub case: Case,
    /// Corrupted observation which must differ from the control and be rejected.
    pub observed: Observation,
}

/// Fifty distinct observation defects, with twelve deduplicated exact controls.
///
/// These corrupt the independent observations only. They neither execute nor
/// replace the product parser, and are not product-mutation execution evidence.
pub fn known_bad_observations() -> Vec<KnownBad> {
    let mut controls = structural_cases();
    controls.extend(raw_payload_cases());
    controls.extend(diagnostic_cases());
    let get = |id: &str| {
        controls
            .iter()
            .find(|case| case.id == id)
            .expect("literal control")
    };
    let select = get("select/quoted-public-trivia");
    let defaults = get("select/options/0/7");
    let update = get("update/markers-across-where");
    let delete = get("delete/where");
    let table = get("ddl/constraints/True/True");
    let index = get("index/targets/True/True/True");
    let drop_table = get("drop/False/False");
    let drop_index = get("drop/True/False");
    let utf8 = get("raw/type-length/Utf8/plus-trivia");
    let binary = get("raw/type-length/Binary/negative");
    let unsupported = get("syntax/structure/18");
    let name_error = get("diagnostic/name/53454c45435420/2222");
    let mut cases = Vec::new();
    let mut push = |id, case: &Case, mutate: fn(&mut Observation)| {
        let mut observed = case.expected.clone();
        mutate(&mut observed);
        cases.push(KnownBad {
            id,
            case: case.clone(),
            observed,
        });
    };
    push("route-select", select, |o| o.nodes[0].kind = Kind::Update);
    push("route-update", update, |o| o.nodes[0].kind = Kind::Delete);
    push("route-delete", delete, |o| o.nodes[0].kind = Kind::Update);
    push("route-create-table", table, |o| {
        o.nodes[0].kind = Kind::DropTable
    });
    push("index-unique-flag", index, |o| {
        o.nodes[0].kind = Kind::CreateIndex { unique: false }
    });
    push("route-drop-table", drop_table, |o| {
        o.nodes[0].kind = Kind::DropIndex
    });
    push("route-drop-index", drop_index, |o| {
        o.nodes[0].kind = Kind::DropTable
    });
    push("trimmed-source", select, |o| {
        o.source.remove(0);
    });
    push("extra-source", select, |o| o.source.push(b' '));
    push("root-span-includes-leading-trivia", select, |o| {
        o.nodes[0].span[0] = 0
    });
    push("root-span-includes-semicolon", select, |o| {
        o.nodes[0].span[1] += 1
    });
    push("missing-last-arena-entry", select, |o| {
        o.nodes.pop();
    });
    push("extra-unreachable-arena-entry", select, |o| {
        o.nodes.push(o.nodes[3].clone())
    });
    push("wrong-preorder", select, |o| o.nodes.swap(3, 4));
    push("missing-first-child", select, |o| {
        o.nodes[0].first_child = None
    });
    push("missing-clause-sibling", select, |o| {
        o.nodes[5].next_sibling = None
    });
    push("alias-not-input-as-first-child", select, |o| {
        o.nodes[2].first_child = Some(4)
    });
    push("wrong-marker-total", select, |o| o.marker_count = 2);
    push("limit-resets-marker-ordinal", select, |o| {
        o.nodes[18].kind = Kind::Scalar(ScalarObservation::Parameter { ordinal: 1 })
    });
    push("where-resets-marker-after-set", update, |o| {
        o.nodes[14].kind = Kind::Scalar(ScalarObservation::Parameter { ordinal: 1 })
    });
    push("where-adds-predicate-root", select, |o| {
        o.nodes[9].kind = Kind::Unexpected("Predicate".into())
    });
    push("implicit-direction-defaulted", defaults, |o| {
        o.nodes[13].kind = Kind::OrderTerm {
            direction: Some(Direction::Asc),
            nulls: None,
        }
    });
    push("implicit-nulls-defaulted", defaults, |o| {
        o.nodes[13].kind = Kind::OrderTerm {
            direction: None,
            nulls: Some(NullPlacement::First),
        }
    });
    push("explicit-direction-reversed", select, |o| {
        o.nodes[15].kind = Kind::OrderTerm {
            direction: Some(Direction::Asc),
            nulls: Some(NullPlacement::First),
        }
    });
    push("explicit-nulls-reversed", select, |o| {
        o.nodes[15].kind = Kind::OrderTerm {
            direction: Some(Direction::Desc),
            nulls: Some(NullPlacement::Last),
        }
    });
    push("wrong-normalized-name", select, |o| {
        if let Kind::Name(n) = &mut o.nodes[3].kind {
            n.bytes = b"X".to_vec();
        }
    });
    push("lost-name-quoted-flag", select, |o| {
        if let Kind::Name(n) = &mut o.nodes[3].kind {
            n.quoted = false;
        }
    });
    push("name-payload-span-differs", select, |o| {
        if let Kind::Name(n) = &mut o.nodes[3].kind {
            n.span[0] += 1;
        }
    });
    push("implicit-nullability-defaulted", table, |o| {
        o.nodes[4].kind = Kind::ColumnDefinition {
            nullability: Nullability::NotNull,
        }
    });
    push("explicit-nullability-erased", table, |o| {
        o.nodes[7].kind = Kind::ColumnDefinition {
            nullability: Nullability::Implicit,
        }
    });
    push("primary-key-becomes-unique", table, |o| {
        o.nodes[10].kind = Kind::UniqueConstraint
    });
    push("storage-disk-erased", table, |o| {
        o.nodes[20].kind = Kind::Unexpected("StorageMemory".into())
    });
    push("table-span-loses-opening-parenthesis", table, |o| {
        o.nodes[3].span[0] += 1
    });
    push("type-length-plus-erased", utf8, |o| {
        if let Kind::DataType(DataType::Utf8(n)) = &mut o.nodes[6].kind {
            n.sign = "None";
        }
    });
    push("type-length-digits-trimmed", utf8, |o| {
        if let Kind::DataType(DataType::Utf8(n)) = &mut o.nodes[6].kind {
            n.digits[0] += 1;
        }
    });
    push("type-length-span-loses-sign-trivia", utf8, |o| {
        if let Kind::DataType(DataType::Utf8(n)) = &mut o.nodes[6].kind {
            n.span = n.digits;
        }
    });
    push("varchar-becomes-varbinary", utf8, |o| {
        if let Kind::DataType(DataType::Utf8(n)) = o.nodes[6].kind {
            o.nodes[6].kind = Kind::DataType(DataType::Binary(n));
        }
    });
    push("type-length-minus-erased", binary, |o| {
        if let Kind::DataType(DataType::Binary(n)) = &mut o.nodes[6].kind {
            n.sign = "None";
        }
    });
    push("negative-type-length-source-normalized", binary, |o| {
        o.source[27] = b'+'
    });
    push("bigint-becomes-boolean", table, |o| {
        o.nodes[6].kind = Kind::DataType(DataType::Bool)
    });
    push("index-table-target-swapped", index, |o| {
        o.nodes[0].first_child = Some(4)
    });
    push("unsupported-becomes-syntax", unsupported, |o| {
        o.error.as_mut().expect("error").code = "Syntax"
    });
    push("name-module-becomes-parse", name_error, |o| {
        o.error.as_mut().expect("error").module = "Parse"
    });
    push("error-span-trimmed", name_error, |o| {
        o.error.as_mut().expect("error").span[1] -= 1
    });
    push("error-leaks-partial-arena", name_error, |o| {
        o.nodes.push(node(Kind::Select, [0, 6], None, None))
    });
    push("error-leaks-source", name_error, |o| o.source.push(b' '));
    push("error-leaks-markers", name_error, |o| o.marker_count = 1);
    push("incomplete-observation", select, |o| o.complete = false);
    push("missing-diagnostic", name_error, |o| o.error = None);
    push("spurious-diagnostic", select, |o| {
        o.error = Some(ErrorObservation {
            code: "Syntax",
            module: "Parse",
            span: [0, 0],
        })
    });
    cases
}

fn node(kind: Kind, span: [usize; 2], first: Option<usize>, next: Option<usize>) -> Node {
    Node {
        kind,
        span,
        first_child: first,
        next_sibling: next,
    }
}

fn good(
    id: impl Into<String>,
    family: &'static str,
    source: &[u8],
    nodes: Vec<Node>,
    markers: u16,
) -> Case {
    Case {
        id: id.into(),
        family,
        source: source.to_vec(),
        expected: Observation {
            source: source.to_vec(),
            nodes,
            marker_count: markers,
            error: None,
            complete: true,
        },
    }
}

fn bad(
    id: impl Into<String>,
    family: &'static str,
    source: &[u8],
    code: &'static str,
    module: &'static str,
    span: [usize; 2],
) -> Case {
    Case {
        id: id.into(),
        family,
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
