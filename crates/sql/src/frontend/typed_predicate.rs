//! Complete static predicate validation and allocation-free borrowed evaluation.
//!
//! Explicit column metadata is a pure input, not a catalog or prepared binding.
//! Compiled outputs own their literals and metadata on the caller's SQL ledgers.

mod budget;
mod compile;
mod eval;

use super::{
    CallControl, Code, ComparisonOp, DeclaredType, Diagnostic, Module, Name, OwnedScalar,
    ParsedStatement, ScalarReader, ScalarRef, ScalarSpec, ScalarType, Span,
};
use budget::{ChargedVec, TypedBudget};
use foundation::{runtime::Quota, value::MAX_VALUE_BYTES};

const MAX_COLUMNS: usize = 32;
const MAX_MARKERS: usize = 256;
const MAX_NODES: usize = 8192;

/// One caller-supplied column, using an already validated canonical name.
pub struct ColumnSpec<'a> {
    /// Name identity; its diagnostic span may belong to a different source.
    pub name: &'a Name,
    /// Declared type, byte bound and nullability, validated for every compile.
    pub spec: ScalarSpec,
}

/// Result type of a compiled syntax node, separate from SQL scalar booleans.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpressionType {
    /// A three-valued predicate result.
    Truth,
    /// A concrete SQL scalar type, including that type's NULL values.
    Scalar(ScalarType),
    /// Raw SQL NULL, with no invented default type.
    UntypedNull,
}

/// Read-only node role. Literal bytes borrow the owning compiled predicate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompiledKindRef<'a> {
    /// Standalone predicate root.
    Predicate,
    /// Ordered disjunction children, including a singleton.
    Or,
    /// Ordered conjunction children, including a singleton.
    And,
    /// Negation of one predicate child.
    Not,
    /// Parenthesized predicate.
    Group,
    /// A comparison whose operand type is known unless both sides are raw NULL.
    Compare {
        /// Original supported comparison operator.
        op: ComparisonOp,
        /// Resolved operand type; None only for two untyped NULL operands.
        scalar_type: Option<ScalarType>,
    },
    /// A null test, always returning True or False.
    IsNull {
        /// Whether the source used IS NOT NULL.
        negated: bool,
    },
    /// A resolved column reference.
    Column {
        /// Zero-based position in the compiled schema and native row.
        ordinal: u8,
    },
    /// Independently owned, decoded SQL literal exposed through a borrowed view.
    Literal(ScalarRef<'a>),
    /// Raw untyped SQL NULL.
    Null,
    /// A resolved parameter occurrence.
    Parameter {
        /// One-based source ordinal, without renumbering inside groups.
        ordinal: u16,
    },
}

#[derive(Debug)]
enum CompiledKind {
    Predicate,
    Or,
    And,
    Not,
    Group,
    Compare {
        op: ComparisonOp,
        scalar_type: Option<ScalarType>,
    },
    IsNull {
        negated: bool,
    },
    Column {
        ordinal: u8,
    },
    Literal(OwnedScalar),
    Null,
    Parameter {
        ordinal: u16,
    },
}

/// One immutable typed node, preserving raw preorder links and source span.
#[derive(Debug)]
pub struct CompiledNode {
    kind: CompiledKind,
    result_type: ExpressionType,
    span: Span,
    first_child: Option<usize>,
    next_sibling: Option<usize>,
}

impl CompiledNode {
    /// Borrow the node's role and any literal payload without releasing leases.
    pub fn kind(&self) -> CompiledKindRef<'_> {
        match &self.kind {
            CompiledKind::Predicate => CompiledKindRef::Predicate,
            CompiledKind::Or => CompiledKindRef::Or,
            CompiledKind::And => CompiledKindRef::And,
            CompiledKind::Not => CompiledKindRef::Not,
            CompiledKind::Group => CompiledKindRef::Group,
            CompiledKind::Compare { op, scalar_type } => CompiledKindRef::Compare {
                op: *op,
                scalar_type: *scalar_type,
            },
            CompiledKind::IsNull { negated } => CompiledKindRef::IsNull { negated: *negated },
            CompiledKind::Column { ordinal } => CompiledKindRef::Column { ordinal: *ordinal },
            CompiledKind::Literal(value) => CompiledKindRef::Literal(value.as_ref()),
            CompiledKind::Null => CompiledKindRef::Null,
            CompiledKind::Parameter { ordinal } => CompiledKindRef::Parameter { ordinal: *ordinal },
        }
    }
    /// Read the scalar, untyped-NULL, or predicate result category.
    pub fn result_type(&self) -> ExpressionType {
        self.result_type
    }
    /// Original UTF-8 byte interval in the caller's parsed source.
    pub fn span(&self) -> Span {
        self.span
    }
    /// First child index in this complete immutable arena.
    pub fn first_child(&self) -> Option<usize> {
        self.first_child
    }
    /// Next child of the same parent, in source order.
    pub fn next_sibling(&self) -> Option<usize> {
        self.next_sibling
    }
}

/// Independently owned schema metadata; it is not a catalog object identity.
#[derive(Debug)]
pub struct CompiledColumn {
    name: [u8; 128],
    len: usize,
    quoted: bool,
    spec: ScalarSpec,
    first_use: Option<Span>,
}

impl CompiledColumn {
    /// Canonical validated name bytes, without delimiters.
    pub fn name_bytes(&self) -> &[u8] {
        &self.name[..self.len]
    }
    /// Original name quoting metadata; identity still uses canonical bytes only.
    pub fn is_quoted(&self) -> bool {
        self.quoted
    }
    /// Validated destination metadata for every supplied native row value.
    pub fn spec(&self) -> ScalarSpec {
        self.spec
    }
    /// First source reference, or None for a column unused by this predicate.
    pub fn first_use(&self) -> Option<Span> {
        self.first_use
    }
}

/// Fully resolved type metadata for one parameter occurrence.
#[derive(Debug)]
pub struct ParameterMetadata {
    ordinal: u16,
    spec: ScalarSpec,
    span: Span,
}

impl ParameterMetadata {
    /// One-based occurrence ordinal.
    pub fn ordinal(&self) -> u16 {
        self.ordinal
    }
    /// Predicate parameters are nullable and variable types allow 65,536 bytes.
    pub fn spec(&self) -> ScalarSpec {
        self.spec
    }
    /// Original marker token span.
    pub fn span(&self) -> Span {
        self.span
    }
}

/// SQL's three predicate outcomes, distinct from scalar Bool and index ordering.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TruthValue {
    /// Known true.
    True,
    /// Known false.
    False,
    /// Unknown because of SQL NULL semantics.
    Unknown,
}

impl TruthValue {
    /// WHERE retains only known true, never false or unknown.
    pub fn passes_where(self) -> bool {
        self == Self::True
    }
}

/// Reusable compiler sharing existing session and database SQL quota owners.
pub struct PredicateCompiler {
    budget: TypedBudget,
    reader: ScalarReader,
}

impl PredicateCompiler {
    /// Use caller ledgers; constructing this compiler makes no backing allocation.
    pub fn new(session: &Quota, database: &Quota) -> Self {
        Self {
            budget: TypedBudget::new(session, database),
            reader: ScalarReader::new(session, database),
        }
    }

    /// Fail a one-based backing site in the next compile call only.
    /// Zero disables injection; early failure also consumes the pending setting.
    #[cfg(feature = "test-support")]
    pub fn fail_allocation_at(&mut self, ordinal: usize) {
        self.budget.fail_allocation_at(ordinal);
    }

    /// Add elements to typed backing requests and bytes to scalar payload requests.
    /// Padding persists until reset to zero, and never affects zero-length sites.
    #[cfg(feature = "test-support")]
    pub fn set_allocation_padding(&mut self, elements: usize) {
        self.budget.set_allocation_padding(elements);
        self.reader.set_allocation_padding(elements);
    }

    /// Backing sites reached after both base reservations in the latest compile.
    #[cfg(feature = "test-support")]
    pub fn allocation_count(&self) -> usize {
        self.budget.allocation_count()
    }
}

/// Immutable typed syntax, constants and schema with independent quota ownership.
/// No source, control, row, parameter or compiler borrow survives construction.
pub struct CompiledPredicate {
    nodes: ChargedVec<CompiledNode>,
    columns: ChargedVec<CompiledColumn>,
    parameters: ChargedVec<ParameterMetadata>,
    span: Span,
}

impl CompiledPredicate {
    /// Complete typed preorder arena with a unique root at index zero.
    pub fn nodes(&self) -> &[CompiledNode] {
        &self.nodes.values
    }
    /// All validated schema entries, including unused columns.
    pub fn columns(&self) -> &[CompiledColumn] {
        &self.columns.values
    }
    /// Resolved metadata in one-based marker order.
    pub fn parameters(&self) -> &[ParameterMetadata] {
        &self.parameters.values
    }
    /// Original predicate span, excluding outer trivia and a final semicolon.
    pub fn span(&self) -> Span {
        self.span
    }
    /// Total actual retained typed backing and literal payload bytes.
    #[cfg(feature = "test-support")]
    pub fn capacity_bytes(&self) -> usize {
        self.node_capacity_bytes()
            + self.column_capacity_bytes()
            + self.parameter_capacity_bytes()
            + self.literal_capacity_bytes()
    }
    /// Actual typed node arena bytes.
    #[cfg(feature = "test-support")]
    pub fn node_capacity_bytes(&self) -> usize {
        self.nodes.capacity_bytes()
    }
    /// Actual schema arena bytes.
    #[cfg(feature = "test-support")]
    pub fn column_capacity_bytes(&self) -> usize {
        self.columns.capacity_bytes()
    }
    /// Actual parameter metadata arena bytes.
    #[cfg(feature = "test-support")]
    pub fn parameter_capacity_bytes(&self) -> usize {
        self.parameters.capacity_bytes()
    }
    /// Actual decoded literal backing bytes, excluding fixed inline values.
    #[cfg(feature = "test-support")]
    pub fn literal_capacity_bytes(&self) -> usize {
        self.nodes
            .values
            .iter()
            .map(|node| match &node.kind {
                CompiledKind::Literal(value) => value.capacity_bytes(),
                _ => 0,
            })
            .sum()
    }
}

impl std::fmt::Debug for CompiledPredicate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledPredicate")
            .field("nodes", &self.nodes.values.len())
            .field("columns", &self.columns.values.len())
            .field("parameters", &self.parameters.values.len())
            .field("span", &self.span)
            .finish()
    }
}

const _: () = assert!(std::mem::size_of::<PredicateCompiler>() <= 256);
const _: () = assert!(std::mem::size_of::<CompiledPredicate>() <= 512);
const _: () = assert!(std::mem::size_of::<CompiledNode>() <= 256);
const _: () = assert!(std::mem::size_of::<CompiledColumn>() <= 256);
const _: () = assert!(std::mem::size_of::<ParameterMetadata>() <= 64);
const _: () = assert!(std::mem::size_of::<[TruthValue; MAX_NODES]>() == 8192);

fn point(start: usize) -> Span {
    Span { start, end: start }
}

fn diagnostic(code: Code, module: Module, span: Span) -> Diagnostic {
    Diagnostic { code, module, span }
}

fn declared_type(declared: DeclaredType) -> ScalarType {
    match declared {
        DeclaredType::Int64 => ScalarType::Int64,
        DeclaredType::Bool => ScalarType::Bool,
        DeclaredType::Utf8(_) => ScalarType::Utf8,
        DeclaredType::Binary(_) => ScalarType::Binary,
    }
}

fn scalar_type(value: ScalarRef<'_>) -> ScalarType {
    match value {
        ScalarRef::Null(kind) => kind,
        ScalarRef::Int64(_) => ScalarType::Int64,
        ScalarRef::Bool(_) => ScalarType::Bool,
        ScalarRef::Utf8(_) => ScalarType::Utf8,
        ScalarRef::Binary(_) => ScalarType::Binary,
    }
}

fn parameter_spec(kind: ScalarType) -> ScalarSpec {
    ScalarSpec {
        declared_type: match kind {
            ScalarType::Int64 => DeclaredType::Int64,
            ScalarType::Bool => DeclaredType::Bool,
            ScalarType::Utf8 => DeclaredType::Utf8(MAX_VALUE_BYTES),
            ScalarType::Binary => DeclaredType::Binary(MAX_VALUE_BYTES),
        },
        nullable: true,
    }
}
