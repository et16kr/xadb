//! Whole-input static checks precede every compiled backing allocation.

use super::super::{AstNode, DecodedScalar, NodeKind, ScalarSyntax};
use super::{
    CallControl, ChargedVec, Code, ColumnSpec, CompiledColumn, CompiledKind, CompiledNode,
    CompiledPredicate, DeclaredType, Diagnostic, ExpressionType, MAX_COLUMNS, MAX_MARKERS,
    MAX_VALUE_BYTES, Module, Name, ParameterMetadata, ParsedStatement, PredicateCompiler,
    ScalarType, Span, declared_type, diagnostic, parameter_spec, point, scalar_type,
};

struct Scratch {
    marker_types: [Option<ScalarType>; MAX_MARKERS],
    marker_spans: [Span; MAX_MARKERS],
    first_use: [Option<Span>; MAX_COLUMNS],
}

const _: () = assert!(std::mem::size_of::<Scratch>() <= 8192);

impl PredicateCompiler {
    /// Validate the entire predicate and retain an independent typed plan.
    ///
    /// Names and all concrete type constraints precede unknown-parameter and
    /// literal validation. Every literal is decoded, including inactive branches.
    /// Failed calls release only their new output and consume fault injection.
    pub fn compile(
        &mut self,
        parsed: &ParsedStatement,
        columns: &[ColumnSpec<'_>],
        parameter_hints: Option<&[Option<ScalarType>]>,
        control: &CallControl<'_>,
    ) -> Result<CompiledPredicate, Diagnostic> {
        self.budget.begin_call();
        check(control, 0)?;
        let root = parsed.nodes().first().ok_or_else(|| syntax(point(0)))?;
        if !matches!(root.kind(), NodeKind::Predicate) {
            return Err(syntax(root.span()));
        }
        if columns.len() > MAX_COLUMNS {
            return Err(diagnostic(Code::Limit, Module::Compile, point(0)));
        }
        let markers = usize::from(parsed.marker_count());
        if parameter_hints.is_some_and(|hints| hints.len() != markers) {
            return Err(diagnostic(Code::BindCount, Module::Compile, point(0)));
        }
        validate_columns(columns, control)?;
        let mut scratch = Scratch {
            marker_types: [None; MAX_MARKERS],
            marker_spans: [point(0); MAX_MARKERS],
            first_use: [None; MAX_COLUMNS],
        };
        for ordinal in 0..markers {
            check(control, 0)?;
            scratch.marker_types[ordinal] = parameter_hints.and_then(|hints| hints[ordinal]);
        }
        validate_names(parsed, columns, &mut scratch, control)?;
        validate_types(parsed, columns, &mut scratch, control)?;
        for ordinal in 0..markers {
            let span = scratch.marker_spans[ordinal];
            check(control, span.start)?;
            if scratch.marker_types[ordinal].is_none() {
                return Err(diagnostic(
                    Code::ParameterTypeUnknown,
                    Module::Compile,
                    span,
                ));
            }
        }
        self.own_plan(parsed, columns, &scratch, control)
    }

    fn own_plan(
        &mut self,
        parsed: &ParsedStatement,
        columns: &[ColumnSpec<'_>],
        scratch: &Scratch,
        control: &CallControl<'_>,
    ) -> Result<CompiledPredicate, Diagnostic> {
        // Frozen site order: schema, parameter metadata, complete typed arena.
        check(control, 0)?;
        let mut owned_columns = ChargedVec::allocate(&mut self.budget, columns.len())?;
        check(control, 0)?;
        let mut parameters =
            ChargedVec::allocate(&mut self.budget, usize::from(parsed.marker_count()))?;
        check(control, 0)?;
        let mut nodes = ChargedVec::allocate(&mut self.budget, parsed.nodes().len())?;
        for (ordinal, column) in columns.iter().enumerate() {
            check(control, 0)?;
            let bytes = column.name.as_bytes();
            let mut name = [0; 128];
            name[..bytes.len()].copy_from_slice(bytes);
            owned_columns.values.push(CompiledColumn {
                name,
                len: bytes.len(),
                quoted: column.name.is_quoted(),
                spec: column.spec,
                first_use: scratch.first_use[ordinal],
            });
            check(control, 0)?;
        }
        for ordinal in 1..=parsed.marker_count() {
            let index = usize::from(ordinal - 1);
            let span = scratch.marker_spans[index];
            check(control, span.start)?;
            let kind = scratch.marker_types[index].ok_or_else(|| syntax(span))?;
            parameters.values.push(ParameterMetadata {
                ordinal,
                spec: parameter_spec(kind),
                span,
            });
        }
        for (index, raw) in parsed.nodes().iter().enumerate() {
            check(control, raw.span().start)?;
            let (kind, result_type) = self.own_node(parsed, index, columns, scratch, control)?;
            nodes.values.push(CompiledNode {
                kind,
                result_type,
                span: raw.span(),
                first_child: raw.first_child(),
                next_sibling: raw.next_sibling(),
            });
        }
        let span = parsed.nodes()[0].span();
        check(control, span.end)?;
        Ok(CompiledPredicate {
            nodes,
            columns: owned_columns,
            parameters,
            span,
        })
    }

    fn own_node(
        &mut self,
        parsed: &ParsedStatement,
        index: usize,
        columns: &[ColumnSpec<'_>],
        scratch: &Scratch,
        control: &CallControl<'_>,
    ) -> Result<(CompiledKind, ExpressionType), Diagnostic> {
        let raw = &parsed.nodes()[index];
        let kind = match raw.kind() {
            NodeKind::Predicate => CompiledKind::Predicate,
            NodeKind::Or => CompiledKind::Or,
            NodeKind::And => CompiledKind::And,
            NodeKind::Not => CompiledKind::Not,
            NodeKind::Group => CompiledKind::Group,
            NodeKind::Compare(op) => {
                let (left, right) = operands(parsed, raw)?;
                let scalar_type = raw_type(left, columns, scratch, control)?
                    .or(raw_type(right, columns, scratch, control)?);
                CompiledKind::Compare {
                    op: *op,
                    scalar_type,
                }
            }
            NodeKind::IsNull { negated } => CompiledKind::IsNull { negated: *negated },
            NodeKind::Name(name) => {
                let ordinal = lookup(name, raw.span(), columns, control)?;
                return Ok((
                    CompiledKind::Column {
                        ordinal: ordinal as u8,
                    },
                    ExpressionType::Scalar(declared_type(columns[ordinal].spec.declared_type)),
                ));
            }
            NodeKind::Scalar(ScalarSyntax::Null) => {
                return Ok((CompiledKind::Null, ExpressionType::UntypedNull));
            }
            NodeKind::Scalar(ScalarSyntax::Parameter { ordinal }) => {
                let kind = scratch.marker_types[usize::from(*ordinal - 1)]
                    .ok_or_else(|| syntax(raw.span()))?;
                return Ok((
                    CompiledKind::Parameter { ordinal: *ordinal },
                    ExpressionType::Scalar(kind),
                ));
            }
            NodeKind::Scalar(_) => {
                #[cfg(feature = "test-support")]
                self.reader
                    .fail_allocation_at(self.budget.remaining_failure());
                let decoded = self.reader.decode(parsed, index, control);
                // Include a failed payload site before propagating its Value error.
                #[cfg(feature = "test-support")]
                self.budget
                    .include_scalar_allocations(self.reader.allocation_count())?;
                let DecodedScalar::Value(value) = decoded? else {
                    return Err(syntax(raw.span()));
                };
                let result_type = ExpressionType::Scalar(scalar_type(value.as_ref()));
                return Ok((CompiledKind::Literal(value), result_type));
            }
            _ => return Err(syntax(raw.span())),
        };
        Ok((kind, ExpressionType::Truth))
    }
}

fn validate_columns(
    columns: &[ColumnSpec<'_>],
    control: &CallControl<'_>,
) -> Result<(), Diagnostic> {
    for (index, column) in columns.iter().enumerate() {
        check(control, 0)?;
        let invalid = match column.spec.declared_type {
            DeclaredType::Utf8(bound) | DeclaredType::Binary(bound) => {
                bound == 0 || bound > MAX_VALUE_BYTES
            }
            DeclaredType::Int64 | DeclaredType::Bool => false,
        };
        // Postcheck takes precedence over the pending metadata Range result.
        check(control, 0)?;
        if invalid {
            return Err(diagnostic(Code::Range, Module::Compile, point(0)));
        }
        for prior in &columns[..index] {
            check(control, 0)?;
            let duplicate = column.name.as_bytes() == prior.name.as_bytes();
            // A candidate compares at most 128 pairs: 256 inspected bytes.
            check(control, 0)?;
            if duplicate {
                return Err(diagnostic(Code::Name, Module::Compile, point(0)));
            }
        }
    }
    Ok(())
}

fn validate_names(
    parsed: &ParsedStatement,
    columns: &[ColumnSpec<'_>],
    scratch: &mut Scratch,
    control: &CallControl<'_>,
) -> Result<(), Diagnostic> {
    for node in parsed.nodes() {
        check(control, node.span().start)?;
        match node.kind() {
            NodeKind::Name(name) => {
                let ordinal = lookup(name, node.span(), columns, control)?;
                scratch.first_use[ordinal].get_or_insert(node.span());
            }
            NodeKind::Scalar(ScalarSyntax::Parameter { ordinal }) => {
                scratch.marker_spans[usize::from(*ordinal - 1)] = node.span();
            }
            NodeKind::Predicate
            | NodeKind::Or
            | NodeKind::And
            | NodeKind::Not
            | NodeKind::Group
            | NodeKind::Compare(_)
            | NodeKind::IsNull { .. }
            | NodeKind::Scalar(_) => {}
            _ => return Err(syntax(node.span())),
        }
    }
    Ok(())
}

fn validate_types(
    parsed: &ParsedStatement,
    columns: &[ColumnSpec<'_>],
    scratch: &mut Scratch,
    control: &CallControl<'_>,
) -> Result<(), Diagnostic> {
    for node in parsed.nodes() {
        check(control, node.span().start)?;
        if let NodeKind::Compare(_) = node.kind() {
            let (left, right) = operands(parsed, node)?;
            let left_type = raw_type(left, columns, scratch, control)?;
            let right_type = raw_type(right, columns, scratch, control)?;
            match (left_type, right_type) {
                (Some(left_type), Some(right_type)) if left_type != right_type => {
                    let span = match (marker(left), marker(right)) {
                        (Some(a), Some(b)) => scratch.marker_spans[usize::from(a.min(b) - 1)],
                        (Some(_), None) => left.span(),
                        (None, Some(_)) => right.span(),
                        (None, None) => node.span(),
                    };
                    return Err(diagnostic(Code::Type, Module::Compile, span));
                }
                (Some(kind), None) => infer_marker(right, kind, scratch),
                (None, Some(kind)) => infer_marker(left, kind, scratch),
                _ => {}
            }
        }
    }
    Ok(())
}

fn infer_marker(node: &AstNode, kind: ScalarType, scratch: &mut Scratch) {
    if let Some(ordinal) = marker(node) {
        scratch.marker_types[usize::from(ordinal - 1)] = Some(kind);
    }
}

fn marker(node: &AstNode) -> Option<u16> {
    match node.kind() {
        NodeKind::Scalar(ScalarSyntax::Parameter { ordinal }) => Some(*ordinal),
        _ => None,
    }
}

fn raw_type(
    node: &AstNode,
    columns: &[ColumnSpec<'_>],
    scratch: &Scratch,
    control: &CallControl<'_>,
) -> Result<Option<ScalarType>, Diagnostic> {
    check(control, node.span().start)?;
    Ok(match node.kind() {
        NodeKind::Name(name) => {
            let ordinal = lookup(name, node.span(), columns, control)?;
            Some(declared_type(columns[ordinal].spec.declared_type))
        }
        NodeKind::Scalar(ScalarSyntax::Integer { .. }) => Some(ScalarType::Int64),
        NodeKind::Scalar(ScalarSyntax::Text) => Some(ScalarType::Utf8),
        NodeKind::Scalar(ScalarSyntax::Binary) => Some(ScalarType::Binary),
        NodeKind::Scalar(ScalarSyntax::Bool(_)) => Some(ScalarType::Bool),
        NodeKind::Scalar(ScalarSyntax::Null) => None,
        NodeKind::Scalar(ScalarSyntax::Parameter { ordinal }) => {
            scratch.marker_types[usize::from(*ordinal - 1)]
        }
        _ => return Err(syntax(node.span())),
    })
}

fn lookup(
    name: &Name,
    span: Span,
    columns: &[ColumnSpec<'_>],
    control: &CallControl<'_>,
) -> Result<usize, Diagnostic> {
    for (ordinal, column) in columns.iter().enumerate() {
        check(control, span.start)?;
        let equal = name.as_bytes() == column.name.as_bytes();
        check(control, span.start)?;
        if equal {
            return Ok(ordinal);
        }
    }
    Err(diagnostic(Code::Name, Module::Compile, span))
}

fn operands<'a>(
    parsed: &'a ParsedStatement,
    node: &AstNode,
) -> Result<(&'a AstNode, &'a AstNode), Diagnostic> {
    let left = node
        .first_child()
        .and_then(|index| parsed.nodes().get(index))
        .ok_or_else(|| syntax(node.span()))?;
    let right = left
        .next_sibling()
        .and_then(|index| parsed.nodes().get(index))
        .ok_or_else(|| syntax(node.span()))?;
    Ok((left, right))
}

fn check(control: &CallControl<'_>, position: usize) -> Result<(), Diagnostic> {
    control.check(Module::Compile, point(position))
}

fn syntax(span: Span) -> Diagnostic {
    diagnostic(Code::Syntax, Module::Compile, span)
}
