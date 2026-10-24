//! Borrowed whole-input validation and a fixed-stack eager SQL truth interpreter.

use std::cmp::Ordering;

use super::{
    CallControl, Code, ComparisonOp, CompiledKind, CompiledNode, CompiledPredicate, DeclaredType,
    Diagnostic, MAX_NODES, MAX_VALUE_BYTES, Module, ScalarRef, ScalarSpec, Span, TruthValue,
    declared_type, diagnostic, point, scalar_type,
};

#[derive(Clone, Copy)]
enum Value<'a> {
    UntypedNull,
    Typed(ScalarRef<'a>),
}

impl Value<'_> {
    fn is_null(self) -> bool {
        matches!(self, Self::UntypedNull | Self::Typed(ScalarRef::Null(_)))
    }
}

impl CompiledPredicate {
    /// Validate every borrowed row field and SQL parameter, then evaluate SQL 3VL.
    ///
    /// Native empty text remains a value; SQL parameter empty text becomes a typed
    /// NULL view. This call owns no inputs, allocates no heap, and retains no control.
    pub fn evaluate(
        &self,
        native_row: &[ScalarRef<'_>],
        sql_parameters: &[ScalarRef<'_>],
        control: &CallControl<'_>,
    ) -> Result<TruthValue, Diagnostic> {
        // One entry point0; fixed scratch checks below use the source root start.
        check(control, 0)?;
        if native_row.len() != self.columns.values.len() {
            return Err(diagnostic(Code::RowCount, Module::Eval, point(0)));
        }
        if sql_parameters.len() != self.parameters.values.len() {
            return Err(diagnostic(Code::BindCount, Module::Eval, point(0)));
        }
        self.validate_tags(native_row, sql_parameters, control)?;
        self.validate_values(native_row, sql_parameters, control)?;
        self.evaluate_nodes(native_row, sql_parameters, control)
    }

    fn validate_tags(
        &self,
        native_row: &[ScalarRef<'_>],
        sql_parameters: &[ScalarRef<'_>],
        control: &CallControl<'_>,
    ) -> Result<(), Diagnostic> {
        // All row tags, then all parameter tags, before any NULL/length result.
        for (value, column) in native_row.iter().zip(&self.columns.values) {
            let span = column.first_use.unwrap_or(point(0));
            check(control, span.start)?;
            validate_tag(*value, column.spec, span)?;
        }
        for (value, parameter) in sql_parameters.iter().zip(&self.parameters.values) {
            check(control, parameter.span.start)?;
            validate_tag(*value, parameter.spec, parameter.span)?;
        }
        Ok(())
    }

    fn validate_values(
        &self,
        native_row: &[ScalarRef<'_>],
        sql_parameters: &[ScalarRef<'_>],
        control: &CallControl<'_>,
    ) -> Result<(), Diagnostic> {
        for (value, column) in native_row.iter().zip(&self.columns.values) {
            let span = column.first_use.unwrap_or(point(0));
            check(control, span.start)?;
            validate_value(*value, column.spec, span)?;
        }
        for (value, parameter) in sql_parameters.iter().zip(&self.parameters.values) {
            check(control, parameter.span.start)?;
            validate_value(normalize_parameter(*value), parameter.spec, parameter.span)?;
        }
        Ok(())
    }

    fn evaluate_nodes(
        &self,
        native_row: &[ScalarRef<'_>],
        sql_parameters: &[ScalarRef<'_>],
        control: &CallControl<'_>,
    ) -> Result<TruthValue, Diagnostic> {
        check(control, self.span.start)?;
        // Input-independent 8 KiB setup, bounded separately from payload loops.
        let mut truth = [TruthValue::Unknown; MAX_NODES];
        check(control, self.span.start)?;
        // Parent-before-child construction makes each child's result ready here.
        // No call frame or heap object is created per source nesting level.
        for (index, node) in self.nodes.values.iter().enumerate().rev() {
            check(control, node.span.start)?;
            truth[index] = match node.kind {
                CompiledKind::Predicate | CompiledKind::Group => truth[child(node)?],
                CompiledKind::Not => negate(truth[child(node)?]),
                CompiledKind::Or => self.boolean_children(node, &truth, false, control)?,
                CompiledKind::And => self.boolean_children(node, &truth, true, control)?,
                CompiledKind::Compare { op, .. } => {
                    let left = child(node)?;
                    let right = self.nodes.values[left]
                        .next_sibling
                        .ok_or_else(|| syntax(node.span))?;
                    compare(
                        self.operand(left, native_row, sql_parameters)?,
                        self.operand(right, native_row, sql_parameters)?,
                        op,
                        node.span,
                        control,
                    )?
                }
                CompiledKind::IsNull { negated } => {
                    let is_null = self
                        .operand(child(node)?, native_row, sql_parameters)?
                        .is_null();
                    boolean(is_null != negated)
                }
                CompiledKind::Column { .. }
                | CompiledKind::Parameter { .. }
                | CompiledKind::Literal(_)
                | CompiledKind::Null => TruthValue::Unknown,
            };
        }
        check(control, self.span.end)?;
        Ok(truth[0])
    }

    fn boolean_children(
        &self,
        node: &CompiledNode,
        truth: &[TruthValue; MAX_NODES],
        conjunction: bool,
        control: &CallControl<'_>,
    ) -> Result<TruthValue, Diagnostic> {
        let mut value = boolean(conjunction);
        let mut next = node.first_child;
        while let Some(index) = next {
            let child = &self.nodes.values[index];
            check(control, child.span.start)?;
            // Eager: no absorbing result may skip a later child/checkpoint.
            value = if conjunction {
                and(value, truth[index])
            } else {
                or(value, truth[index])
            };
            next = child.next_sibling;
        }
        Ok(value)
    }

    fn operand<'a>(
        &'a self,
        index: usize,
        native_row: &[ScalarRef<'a>],
        sql_parameters: &[ScalarRef<'a>],
    ) -> Result<Value<'a>, Diagnostic> {
        let node = &self.nodes.values[index];
        Ok(match &node.kind {
            CompiledKind::Column { ordinal } => Value::Typed(native_row[usize::from(*ordinal)]),
            CompiledKind::Parameter { ordinal } => Value::Typed(normalize_parameter(
                sql_parameters[usize::from(*ordinal - 1)],
            )),
            CompiledKind::Literal(value) => Value::Typed(value.as_ref()),
            CompiledKind::Null => Value::UntypedNull,
            _ => return Err(syntax(node.span)),
        })
    }
}

fn validate_tag(value: ScalarRef<'_>, spec: ScalarSpec, span: Span) -> Result<(), Diagnostic> {
    if scalar_type(value) != declared_type(spec.declared_type) {
        return Err(diagnostic(Code::Type, Module::Eval, span));
    }
    Ok(())
}

fn validate_value(value: ScalarRef<'_>, spec: ScalarSpec, span: Span) -> Result<(), Diagnostic> {
    if matches!(value, ScalarRef::Null(_)) {
        return if spec.nullable {
            Ok(())
        } else {
            Err(diagnostic(Code::NotNull, Module::Eval, span))
        };
    }
    let length = match value {
        ScalarRef::Utf8(value) => value.len(),
        ScalarRef::Binary(value) => value.len(),
        _ => return Ok(()),
    };
    let bound = match spec.declared_type {
        DeclaredType::Utf8(bound) | DeclaredType::Binary(bound) => bound,
        _ => return Err(diagnostic(Code::Type, Module::Eval, span)),
    };
    if length > MAX_VALUE_BYTES || length > bound {
        return Err(diagnostic(Code::Range, Module::Eval, span));
    }
    Ok(())
}

fn normalize_parameter(value: ScalarRef<'_>) -> ScalarRef<'_> {
    match value {
        ScalarRef::Utf8("") => ScalarRef::Null(super::ScalarType::Utf8),
        _ => value,
    }
}

fn compare(
    left: Value<'_>,
    right: Value<'_>,
    op: ComparisonOp,
    span: Span,
    control: &CallControl<'_>,
) -> Result<TruthValue, Diagnostic> {
    if left.is_null() || right.is_null() {
        return Ok(TruthValue::Unknown);
    }
    let order = match (left, right) {
        (Value::Typed(ScalarRef::Int64(a)), Value::Typed(ScalarRef::Int64(b))) => a.cmp(&b),
        (Value::Typed(ScalarRef::Bool(a)), Value::Typed(ScalarRef::Bool(b))) => a.cmp(&b),
        (Value::Typed(ScalarRef::Utf8(a)), Value::Typed(ScalarRef::Utf8(b))) => {
            compare_bytes(a.as_bytes(), b.as_bytes(), span, control)?
        }
        (Value::Typed(ScalarRef::Binary(a)), Value::Typed(ScalarRef::Binary(b))) => {
            compare_bytes(a, b, span, control)?
        }
        _ => return Err(diagnostic(Code::Type, Module::Eval, span)),
    };
    Ok(boolean(match op {
        ComparisonOp::Eq => order == Ordering::Equal,
        ComparisonOp::NotEq => order != Ordering::Equal,
        ComparisonOp::Lt => order == Ordering::Less,
        ComparisonOp::Le => order != Ordering::Greater,
        ComparisonOp::Gt => order == Ordering::Greater,
        ComparisonOp::Ge => order != Ordering::Less,
    }))
}

fn compare_bytes(
    left: &[u8],
    right: &[u8],
    span: Span,
    control: &CallControl<'_>,
) -> Result<Ordering, Diagnostic> {
    check_comparison(control, 0, span)?;
    let common = left.len().min(right.len());
    let mut position = 0;
    while position < common {
        // A pair reads two bytes, so 128 pairs exhaust the 256-byte quantum.
        let end = (position + 128).min(common);
        while position < end {
            let order = left[position].cmp(&right[position]);
            position += 1;
            if order != Ordering::Equal {
                check_comparison(control, position, span)?;
                return Ok(order);
            }
        }
        if position < common {
            check_comparison(control, position, span)?;
        }
    }
    // Includes empty payloads and the final short chunk; no full-slice cmp scan.
    check_comparison(control, position, span)?;
    Ok(left.len().cmp(&right.len()))
}

fn check_comparison(
    control: &CallControl<'_>,
    progress: usize,
    span: Span,
) -> Result<(), Diagnostic> {
    // Observer coordinates are local bytes. Returned failures identify the SQL
    // comparison, without pretending local progress is a source byte offset.
    control
        .check(Module::Eval, point(progress))
        .map_err(|mut error| {
            error.span = span;
            error
        })
}

fn child(node: &CompiledNode) -> Result<usize, Diagnostic> {
    node.first_child.ok_or_else(|| syntax(node.span))
}

fn boolean(value: bool) -> TruthValue {
    if value {
        TruthValue::True
    } else {
        TruthValue::False
    }
}

fn negate(value: TruthValue) -> TruthValue {
    match value {
        TruthValue::True => TruthValue::False,
        TruthValue::False => TruthValue::True,
        TruthValue::Unknown => TruthValue::Unknown,
    }
}

fn and(left: TruthValue, right: TruthValue) -> TruthValue {
    match (left, right) {
        (TruthValue::False, _) | (_, TruthValue::False) => TruthValue::False,
        (TruthValue::True, TruthValue::True) => TruthValue::True,
        _ => TruthValue::Unknown,
    }
}

fn or(left: TruthValue, right: TruthValue) -> TruthValue {
    match (left, right) {
        (TruthValue::True, _) | (_, TruthValue::True) => TruthValue::True,
        (TruthValue::False, TruthValue::False) => TruthValue::False,
        _ => TruthValue::Unknown,
    }
}

fn check(control: &CallControl<'_>, position: usize) -> Result<(), Diagnostic> {
    control.check(Module::Eval, point(position))
}

fn syntax(span: Span) -> Diagnostic {
    diagnostic(Code::Syntax, Module::Eval, span)
}
