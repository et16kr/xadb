//! Owned raw syntax, flat links and quota-accounted source/arena lifetimes.
//!
//! P2 does not turn raw scalar syntax into validated SQL values. Later scalar and
//! binding consumers must validate value ranges, payloads and schema constraints.

use super::parse_budget::{ChargedVec, ParseBudget, limit, point};
use super::{CallControl, Diagnostic, Module, Name, Span};

const MAX_AST_NODES: usize = 8192;

/// Raw integer sign, kept separately from decimal digits and intervening trivia.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sign {
    /// No explicit sign token.
    None,
    /// An explicit plus token.
    Plus,
    /// An explicit minus token.
    Minus,
}

/// Explicit ORDER BY direction; omission is retained separately as None.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDirection {
    /// Ascending order.
    Asc,
    /// Descending order.
    Desc,
}

/// Explicit placement of NULL values, without resolving direction defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NullPlacement {
    /// NULL values before non-NULL values.
    First,
    /// NULL values after non-NULL values.
    Last,
}

/// The written column declaration, before primary-key and schema validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NullabilitySyntax {
    /// No nullability clause was written.
    Implicit,
    /// An explicit NULL clause.
    Null,
    /// An explicit NOT NULL clause.
    NotNull,
}

/// Unconverted signed decimal type length; no numeric range is certified.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypeLengthSyntax {
    /// The written sign, if any.
    pub sign: Sign,
    /// Decimal digits alone in the owning statement's source.
    pub digits: Span,
    /// Sign through final digit, including intervening trivia.
    pub span: Span,
}

/// Raw supported type syntax; definition validation belongs after DDL precommit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeSyntax {
    /// BIGINT without a length clause.
    Int64,
    /// BOOLEAN without a length clause.
    Bool,
    /// VARCHAR with an unconverted length.
    Utf8(TypeLengthSyntax),
    /// VARBINARY with an unconverted length.
    Binary(TypeLengthSyntax),
}

/// Unconverted literal or marker syntax. A successful parse is not value validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScalarSyntax {
    /// Raw signed decimal integer; conversion and range checks belong to P3.
    Integer {
        /// Explicit sign, if present.
        sign: Sign,
        /// Digits alone; the node span also includes sign and intervening trivia.
        digits: Span,
    },
    /// Quoted text, retaining raw escapes; empty text is not yet typed NULL.
    Text,
    /// X/x quoted binary payload; hex and decoded length are not yet validated.
    Binary,
    /// A boolean keyword.
    Bool(bool),
    /// An untyped NULL keyword.
    Null,
    /// A marker in source order, starting at one.
    Parameter {
        /// One-based ordinal, at most 256.
        ordinal: u16,
    },
}

/// The six supported comparison operators, without type or value validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComparisonOp {
    /// Equality, written `=`.
    Eq,
    /// Inequality, written `<>`.
    NotEq,
    /// Strictly less than, written `<`.
    Lt,
    /// Less than or equal, written `<=`.
    Le,
    /// Strictly greater than, written `>`.
    Gt,
    /// Greater than or equal, written `>=`.
    Ge,
}

/// Node roles for raw statements and predicate syntax.
#[derive(Debug)]
pub enum NodeKind {
    /// SELECT with Projection, Target and optional Where, OrderBy, Limit.
    Select,
    /// UPDATE with Target, Assignments and optional Where.
    Update,
    /// DELETE with Target and optional Where.
    Delete,
    /// CREATE TABLE with Target, TableDefinition and mandatory StorageDisk.
    CreateTable,
    /// DROP TABLE syntax with one Target.
    DropTable,
    /// CREATE INDEX with index Target, table Target and Columns.
    CreateIndex {
        /// Whether UNIQUE was explicitly written.
        unique: bool,
    },
    /// DROP INDEX syntax with one Target.
    DropIndex,
    /// Either one Wildcard or one to32 ProjectionItem children.
    Projection,
    /// Input column Name and optional explicit AS alias Name.
    ProjectionItem,
    /// A sole projection star.
    Wildcard,
    /// SET clause containing one or more Assignment children.
    Assignments,
    /// Target column Name followed by one raw Scalar.
    Assignment,
    /// WHERE clause with a direct Or child, without a Predicate root.
    Where,
    /// ORDER BY clause containing one to32 OrderTerm children.
    OrderBy,
    /// Input column Name with separately preserved optional modifiers.
    OrderTerm {
        /// Explicit direction, if present.
        direction: Option<SortDirection>,
        /// Explicit NULL placement, if present.
        nulls: Option<NullPlacement>,
    },
    /// LIMIT clause with an unsigned raw Integer or Parameter child.
    Limit,
    /// Parenthesized column definitions, optional PK, then UNIQUE constraints.
    TableDefinition,
    /// Column Name and DataType, without schema or primary-key validation.
    ColumnDefinition {
        /// Exact written nullability option.
        nullability: NullabilitySyntax,
    },
    /// Complete raw type syntax, admitted only after its format is read.
    DataType(TypeSyntax),
    /// Table-level PRIMARY KEY with a Columns child.
    PrimaryKey,
    /// Table-level UNIQUE with a Columns child.
    UniqueConstraint,
    /// Mandatory STORAGE DISK syntax, without a storage side effect.
    StorageDisk,
    /// INSERT root with Target, Columns and Rows children.
    Insert,
    /// BEGIN transaction-control root; no transaction is actually started here.
    Begin,
    /// COMMIT syntax only, without a commit receipt or engine effect.
    Commit,
    /// ROLLBACK syntax only.
    Rollback,
    /// SET AUTOCOMMIT ON/OFF syntax only.
    SetAutocommit(bool),
    /// One name or a PUBLIC prefix followed by a name.
    Target,
    /// Explicit nonempty INSERT or DDL key column list.
    Columns,
    /// VALUES clause with nonempty tuple children.
    Rows,
    /// Nonempty parenthesized scalar list.
    Tuple,
    /// Standalone predicate root with one Or child.
    Predicate,
    /// Nonempty ordered list of And children, including singleton lists.
    Or,
    /// Nonempty ordered list of predicate terms, including singleton lists.
    And,
    /// Predicate negation with one term child.
    Not,
    /// Parenthesized predicate with one Or child; span includes parentheses.
    Group,
    /// Comparison with exactly two Name or Scalar children.
    Compare(ComparisonOp),
    /// Null test with one Name or Scalar child; its NULL is not another child.
    IsNull {
        /// True for IS NOT NULL, false for IS NULL.
        negated: bool,
    },
    /// Fixed-space normalized name with original diagnostic metadata.
    Name(Name),
    /// Raw literal or marker; its source bytes are retained by the statement.
    Scalar(ScalarSyntax),
}

/// One immutable arena node. Local indices are neither handles nor engine identities.
/// All successful trees are rooted at node zero and include every arena entry.
#[derive(Debug)]
pub struct AstNode {
    kind: NodeKind,
    span: Span,
    first_child: Option<usize>,
    last_child: Option<usize>,
    next_sibling: Option<usize>,
}

// A single arena move remains within the frozen 256-byte control quantum.
const _: () = assert!(std::mem::size_of::<AstNode>() <= 256);

impl AstNode {
    /// Read the exact raw syntax role and payload.
    pub fn kind(&self) -> &NodeKind {
        &self.kind
    }
    /// Original local UTF-8 byte interval inside the owning statement's source.
    pub fn span(&self) -> Span {
        self.span
    }
    /// Index of this node's first child, if any.
    pub fn first_child(&self) -> Option<usize> {
        self.first_child
    }
    /// Index of the next child of the same parent, if any.
    pub fn next_sibling(&self) -> Option<usize> {
        self.next_sibling
    }
}

/// One immutable raw syntax tree and source, retaining all backing reservations.
/// It cannot be cloned, detached from its leases, or mutated into an unvalidated tree.
pub struct ParsedStatement {
    source: ChargedVec<u8>,
    nodes: ChargedVec<AstNode>,
    markers: u16,
}

impl ParsedStatement {
    /// Borrow all caller bytes, including trivia and an optional final semicolon.
    pub fn source(&self) -> &[u8] {
        &self.source.values
    }
    /// Borrow the complete preorder arena, with root at index zero.
    pub fn nodes(&self) -> &[AstNode] {
        &self.nodes.values
    }
    /// Number of syntactically permitted markers, before parameter type binding.
    pub fn marker_count(&self) -> u16 {
        self.markers
    }
    /// Actual source allocation bytes, for non-shipping resource observations.
    #[cfg(feature = "test-support")]
    pub fn source_capacity_bytes(&self) -> usize {
        self.source.capacity_bytes()
    }
    /// Actual typed arena allocation bytes, for non-shipping resource observations.
    #[cfg(feature = "test-support")]
    pub fn arena_capacity_bytes(&self) -> usize {
        self.nodes.capacity_bytes()
    }
}

impl std::fmt::Debug for ParsedStatement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParsedStatement")
            .field("source_bytes", &self.source.values.len())
            .field("nodes", &self.nodes.values.len())
            .field("markers", &self.markers)
            .finish()
    }
}

pub(crate) struct AstBuilder<'budget> {
    source: ChargedVec<u8>,
    nodes: ChargedVec<AstNode>,
    budget: &'budget mut ParseBudget,
}

impl<'budget> AstBuilder<'budget> {
    pub(crate) fn new(
        input: &[u8],
        budget: &'budget mut ParseBudget,
        control: &CallControl<'_>,
    ) -> Result<Self, Diagnostic> {
        control.check(Module::Parse, point(0))?;
        let mut source = ChargedVec::allocate(budget, input.len(), point(0))?;
        control.check(Module::Parse, point(0))?;
        for chunk in input.chunks(256) {
            // The entire input capacity was reserved before any copy/publication.
            source.values.extend_from_slice(chunk);
            control.check(Module::Parse, point(source.values.len()))?;
        }
        Ok(Self {
            source,
            nodes: ChargedVec::new(),
            budget,
        })
    }

    pub(crate) fn add(
        &mut self,
        kind: NodeKind,
        span: Span,
        parent: Option<usize>,
        control: &CallControl<'_>,
    ) -> Result<usize, Diagnostic> {
        let at = point(span.start);
        control.check(Module::Parse, at)?;
        if self.nodes.values.len() == MAX_AST_NODES {
            return Err(limit(at));
        }
        if self.nodes.values.len() == self.nodes.values.capacity() {
            self.grow(control, at)?;
        }
        let index = self.nodes.values.len();
        self.nodes.values.push(AstNode {
            kind,
            span,
            first_child: None,
            last_child: None,
            next_sibling: None,
        });
        if let Some(parent) = parent {
            if let Some(tail) = self.nodes.values[parent].last_child {
                self.nodes.values[tail].next_sibling = Some(index);
            } else {
                self.nodes.values[parent].first_child = Some(index);
            }
            self.nodes.values[parent].last_child = Some(index);
        }
        Ok(index)
    }

    fn grow(&mut self, control: &CallControl<'_>, at: Span) -> Result<(), Diagnostic> {
        let old_capacity = self.nodes.values.capacity();
        let requested = if old_capacity == 0 {
            16
        } else {
            old_capacity
                .checked_mul(2)
                .ok_or_else(|| limit(at))?
                .min(MAX_AST_NODES)
        };
        let mut next = ChargedVec::allocate(self.budget, requested, at)?;
        control.check(Module::Parse, at)?;
        // drain retains the old Vec allocation and all its leases during moves.
        // Fixed nodes have no heap child or recursive Drop, even on interruption.
        for node in self.nodes.values.drain(..) {
            let end = node.span.end;
            next.values.push(node);
            control.check(Module::Parse, point(end))?;
        }
        control.check(Module::Parse, at)?;
        self.nodes = next;
        Ok(())
    }

    pub(crate) fn set_span(&mut self, index: usize, span: Span) {
        self.nodes.values[index].span = span;
    }

    pub(crate) fn set_kind(&mut self, index: usize, kind: NodeKind) {
        self.nodes.values[index].kind = kind;
    }

    pub(crate) fn finish(self, markers: u16) -> ParsedStatement {
        ParsedStatement {
            source: self.source,
            nodes: self.nodes,
            markers,
        }
    }
}
