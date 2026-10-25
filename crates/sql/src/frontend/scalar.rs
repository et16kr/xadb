//! Typed scalar input, bounded raw-literal decoding, and shared quota ownership.
//!
//! These values certify their storage representation, not a whole statement,
//! destination column, parameter binding, or predicate's complete type analysis.

mod budget;

use super::{
    CallControl, Code, Diagnostic, Module, NodeKind, ParsedStatement, ScalarSyntax, Sign, Span,
};
use budget::{Backing, Leases, ScalarBudget};
use foundation::{runtime::Quota, value::MAX_VALUE_BYTES};

/// Original SQL logical type, including the type carried by a NULL value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScalarType {
    /// Signed 64-bit integer.
    Int64,
    /// Boolean value.
    Bool,
    /// UTF-8 text, measured in bytes.
    Utf8,
    /// Arbitrary bytes.
    Binary,
}

/// Borrowed typed input or immutable output view. Construction alone does not
/// validate size, destination metadata, nullability, or quota admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScalarRef<'a> {
    /// A NULL retaining its original logical type.
    Null(ScalarType),
    /// Signed integer input.
    Int64(i64),
    /// Boolean input.
    Bool(bool),
    /// Already valid UTF-8 input, including a possible zero-length native value.
    Utf8(&'a str),
    /// Binary input; zero length is a value rather than NULL.
    Binary(&'a [u8]),
}

impl ScalarRef<'_> {
    fn scalar_type(self) -> ScalarType {
        match self {
            Self::Null(kind) => kind,
            Self::Int64(_) => ScalarType::Int64,
            Self::Bool(_) => ScalarType::Bool,
            Self::Utf8(_) => ScalarType::Utf8,
            Self::Binary(_) => ScalarType::Binary,
        }
    }

    fn byte_len(self) -> usize {
        match self {
            Self::Utf8(text) => text.len(),
            Self::Binary(bytes) => bytes.len(),
            _ => 0,
        }
    }
}

/// Destination type metadata supplied by an explicit consumer, without catalog
/// access. Variable declarations require a byte limit in 1..=65,536.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeclaredType {
    /// Signed 64-bit integer destination.
    Int64,
    /// Boolean destination.
    Bool,
    /// UTF-8 destination with the declared maximum byte length.
    Utf8(usize),
    /// Binary destination with the declared maximum byte length.
    Binary(usize),
}

/// Explicit destination metadata; it is checked anew on each copy call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScalarSpec {
    /// Original logical type and any variable-length byte bound.
    pub declared_type: DeclaredType,
    /// Whether a same-type NULL is permitted.
    pub nullable: bool,
}

enum ScalarData {
    Null(ScalarType),
    Int64(i64),
    Bool(bool),
    Utf8(String),
    Binary(Vec<u8>),
}

/// Immutable, independently owned scalar with its byte backing fully charged.
/// The storage is globally valid; a specific destination still needs its own
/// type/nullability/length checks. No clone or uncharged extraction is provided.
pub struct OwnedScalar {
    // Rust drops fields in declaration order: free backing before quota leases.
    data: ScalarData,
    leases: Leases,
}

impl OwnedScalar {
    fn new(data: ScalarData) -> Self {
        Self {
            data,
            leases: Leases::new(),
        }
    }

    /// Borrow the original type and value without detaching the backing leases.
    pub fn as_ref(&self) -> ScalarRef<'_> {
        match &self.data {
            ScalarData::Null(kind) => ScalarRef::Null(*kind),
            ScalarData::Int64(value) => ScalarRef::Int64(*value),
            ScalarData::Bool(value) => ScalarRef::Bool(*value),
            ScalarData::Utf8(text) => ScalarRef::Utf8(text),
            ScalarData::Binary(bytes) => ScalarRef::Binary(bytes),
        }
    }

    fn capacity(&self) -> usize {
        match &self.data {
            ScalarData::Utf8(text) => text.capacity(),
            ScalarData::Binary(bytes) => bytes.capacity(),
            _ => 0,
        }
    }

    /// Actual retained backing bytes, for non-shipping quota observations.
    #[cfg(feature = "test-support")]
    pub fn capacity_bytes(&self) -> usize {
        self.capacity()
    }
}

impl std::fmt::Debug for OwnedScalar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = self.as_ref();
        f.debug_struct("OwnedScalar")
            .field("kind", &value.scalar_type())
            .field("is_null", &matches!(value, ScalarRef::Null(_)))
            .field("payload_bytes", &value.byte_len())
            .finish()
    }
}

/// Decoded literal or unresolved raw syntax. Public construction is not proof of
/// AST provenance, valid marker numbering, or completed destination validation.
#[derive(Debug)]
pub enum DecodedScalar {
    /// A globally valid typed scalar with independently owned backing.
    Value(OwnedScalar),
    /// Raw SQL NULL, awaiting a type from a later typed consumer.
    Null,
    /// Unresolved marker ordinal from the parsed source, starting at one.
    Parameter(u16),
}

/// Reusable scalar reader sharing the caller's existing SQL quota ledgers.
/// Call controls and source borrows are never retained in this reader or output.
pub struct ScalarReader {
    budget: ScalarBudget,
}

const _: () = assert!(std::mem::size_of::<ScalarReader>() <= 128);
const _: () = assert!(std::mem::size_of::<OwnedScalar>() <= 256);
const _: () = assert!(std::mem::size_of::<DecodedScalar>() <= 272);
const _: () = assert!(std::mem::size_of::<ScalarRef<'_>>() <= 32);

impl ScalarReader {
    /// Share the existing session and database ledgers without creating budgets.
    pub fn new(session: &Quota, database: &Quota) -> Self {
        Self {
            budget: ScalarBudget::new(session, database),
        }
    }

    /// Decode one actual scalar node, preserving unresolved NULL and parameters.
    /// Raw values are checked without applying destination metadata or DB effects.
    ///
    /// # Errors
    /// Returns Syntax for an invalid selection or binary payload, Range for an
    /// overflowing integer or oversized decoded value, Limit on admission or
    /// allocation failure, and the current cancellation/deadline diagnostic.
    pub fn decode(
        &mut self,
        statement: &ParsedStatement,
        node: usize,
        control: &CallControl<'_>,
    ) -> Result<DecodedScalar, Diagnostic> {
        self.budget.begin_call();
        let selected = statement.nodes().get(node);
        let span = selected.map_or(point(statement.source().len()), |node| node.span());
        check(control, span.start)?;
        let selected = selected.ok_or_else(|| diagnostic(Code::Syntax, span))?;
        let NodeKind::Scalar(syntax) = selected.kind() else {
            return Err(diagnostic(Code::Syntax, span));
        };
        let result = match *syntax {
            ScalarSyntax::Integer { sign, digits } => DecodedScalar::Value(OwnedScalar::new(
                ScalarData::Int64(integer(statement.source(), sign, digits, span, control)?),
            )),
            ScalarSyntax::Text => {
                DecodedScalar::Value(self.text(statement.source(), span, control)?)
            }
            ScalarSyntax::Binary => {
                DecodedScalar::Value(self.binary(statement.source(), span, control)?)
            }
            ScalarSyntax::Bool(value) => {
                DecodedScalar::Value(OwnedScalar::new(ScalarData::Bool(value)))
            }
            ScalarSyntax::Null => DecodedScalar::Null,
            ScalarSyntax::Parameter { ordinal } => DecodedScalar::Parameter(ordinal),
        };
        // FINAL_SCALAR_CHECK: includes fixed scalars, raw NULL and markers.
        check(control, span.end)?;
        Ok(result)
    }

    /// Check and copy SQL input, normalizing only same-type empty UTF-8 to NULL.
    /// All returned diagnostic spans preserve `at`; control observers receive
    /// local copied-byte positions rather than SQL source coordinates.
    ///
    /// # Errors
    /// Checks metadata Range, original Type, NotNull and payload Range in that
    /// order; admission/allocation returns Limit. Controls apply throughout.
    pub fn copy_sql(
        &mut self,
        input: ScalarRef<'_>,
        spec: ScalarSpec,
        at: Span,
        control: &CallControl<'_>,
    ) -> Result<OwnedScalar, Diagnostic> {
        self.copy(input, spec, at, control, true)
    }

    /// Check and copy native input while preserving non-NULL empty UTF-8.
    /// Returned diagnostics retain `at`; observers see local byte progress.
    ///
    /// # Errors
    /// Uses the same metadata/type/nullability/length, quota and call-control
    /// errors as `copy_sql`, without SQL empty-text normalization.
    pub fn copy_native(
        &mut self,
        input: ScalarRef<'_>,
        spec: ScalarSpec,
        at: Span,
        control: &CallControl<'_>,
    ) -> Result<OwnedScalar, Diagnostic> {
        self.copy(input, spec, at, control, false)
    }

    /// Fail one backing allocation ordinal in the next decode/copy call.
    /// Zero disables injection; even an early failed call consumes the setting.
    #[cfg(feature = "test-support")]
    pub fn fail_allocation_at(&mut self, ordinal: usize) {
        self.budget.fail_allocation_at(ordinal);
    }

    /// Add bytes to backing allocation requests until explicitly reset to zero.
    /// Empty and fixed values neither apply padding nor allocate.
    #[cfg(feature = "test-support")]
    pub fn set_allocation_padding(&mut self, bytes: usize) {
        self.budget.set_allocation_padding(bytes);
    }

    /// Backing sites reached after both base reservations in the latest call.
    #[cfg(feature = "test-support")]
    pub fn allocation_count(&self) -> usize {
        self.budget.allocation_count()
    }

    fn text(
        &mut self,
        source: &[u8],
        span: Span,
        control: &CallControl<'_>,
    ) -> Result<OwnedScalar, Diagnostic> {
        let length = text_length(source, span, control)?;
        if length == 0 {
            return Ok(OwnedScalar::new(ScalarData::Null(ScalarType::Utf8)));
        }
        let mut output = self
            .budget
            .allocate(Backing::Text, length, point(span.start))?;
        // Charged pass restarts at the original scalar start, after validation
        // has reached the full scalar end. Its progress is then monotonic.
        check(control, span.start)?;
        let ScalarData::Utf8(text) = &mut output.data else {
            unreachable!("text admission supplies String backing");
        };
        let mut position = span.start + 1;
        let mut last_check = span.start;
        while position < span.end - 1 {
            let (piece, next) = text_piece(source, position, span)?;
            // Validation counted this exact payload, so push_str cannot grow.
            text.push_str(piece);
            position = next;
            if position - last_check >= 252 {
                // TEXT_DECODE_CHECK: at most four raw bytes per step.
                check(control, position)?;
                last_check = position;
            }
        }
        check(control, span.end)?;
        Ok(output)
    }

    fn binary(
        &mut self,
        source: &[u8],
        span: Span,
        control: &CallControl<'_>,
    ) -> Result<OwnedScalar, Diagnostic> {
        let length = binary_length(source, span, control)?;
        if length == 0 {
            return Ok(OwnedScalar::new(ScalarData::Binary(Vec::new())));
        }
        let mut output = self
            .budget
            .allocate(Backing::Binary, length, point(span.start))?;
        check(control, span.start)?;
        let ScalarData::Binary(bytes) = &mut output.data else {
            unreachable!("binary admission supplies Vec backing");
        };
        let mut position = span.start + 2;
        let mut last_check = span.start;
        while position < span.end - 1 {
            bytes.push(hex_pair(source, position, span)?);
            position += 2;
            if position - last_check >= 256 {
                // BINARY_DECODE_CHECK: two raw bytes per decoded byte.
                check(control, position)?;
                last_check = position;
            }
        }
        check(control, span.end)?;
        Ok(output)
    }

    fn copy(
        &mut self,
        input: ScalarRef<'_>,
        spec: ScalarSpec,
        at: Span,
        control: &CallControl<'_>,
        sql: bool,
    ) -> Result<OwnedScalar, Diagnostic> {
        self.budget.begin_call();
        copy_check(control, 0, at)?;
        let (expected, maximum) = declared(spec.declared_type, at)?;
        // Preserve original types even for typed NULL and empty SQL text.
        if input.scalar_type() != expected {
            return Err(diagnostic(Code::Type, at));
        }
        let input = if sql && matches!(input, ScalarRef::Utf8("")) {
            ScalarRef::Null(ScalarType::Utf8)
        } else {
            input
        };
        if matches!(input, ScalarRef::Null(_)) && !spec.nullable {
            return Err(diagnostic(Code::NotNull, at));
        }
        let length = input.byte_len();
        if length > MAX_VALUE_BYTES || maximum.is_some_and(|maximum| length > maximum) {
            return Err(diagnostic(Code::Range, at));
        }
        let output = match input {
            ScalarRef::Null(kind) => OwnedScalar::new(ScalarData::Null(kind)),
            ScalarRef::Int64(value) => OwnedScalar::new(ScalarData::Int64(value)),
            ScalarRef::Bool(value) => OwnedScalar::new(ScalarData::Bool(value)),
            ScalarRef::Utf8(text) => self.copy_text(text, at, control)?,
            ScalarRef::Binary(bytes) => self.copy_binary(bytes, at, control)?,
        };
        copy_check(control, length, at)?;
        Ok(output)
    }

    fn copy_text(
        &mut self,
        input: &str,
        at: Span,
        control: &CallControl<'_>,
    ) -> Result<OwnedScalar, Diagnostic> {
        let mut output = self.budget.allocate(Backing::Text, input.len(), at)?;
        copy_check(control, 0, at)?;
        let ScalarData::Utf8(text) = &mut output.data else {
            unreachable!("text admission supplies String backing");
        };
        let mut position = 0;
        while position < input.len() {
            let mut end = (position + 256).min(input.len());
            // A UTF-8 scalar uses at most four bytes; this retreats at most three.
            while !input.is_char_boundary(end) {
                end -= 1;
            }
            text.push_str(&input[position..end]);
            position = end;
            // BORROWED_TEXT_COPY_CHECK: shared SQL/native copy path.
            copy_check(control, position, at)?;
        }
        copy_check(control, input.len(), at)?;
        Ok(output)
    }

    fn copy_binary(
        &mut self,
        input: &[u8],
        at: Span,
        control: &CallControl<'_>,
    ) -> Result<OwnedScalar, Diagnostic> {
        let mut output = self.budget.allocate(Backing::Binary, input.len(), at)?;
        copy_check(control, 0, at)?;
        let ScalarData::Binary(bytes) = &mut output.data else {
            unreachable!("binary admission supplies Vec backing");
        };
        let mut position = 0;
        for chunk in input.chunks(256) {
            bytes.extend_from_slice(chunk);
            position += chunk.len();
            // BORROWED_BINARY_COPY_CHECK: shared SQL/native copy path.
            copy_check(control, position, at)?;
        }
        copy_check(control, input.len(), at)?;
        Ok(output)
    }
}

fn declared(kind: DeclaredType, at: Span) -> Result<(ScalarType, Option<usize>), Diagnostic> {
    match kind {
        DeclaredType::Int64 => Ok((ScalarType::Int64, None)),
        DeclaredType::Bool => Ok((ScalarType::Bool, None)),
        DeclaredType::Utf8(length) | DeclaredType::Binary(length) => {
            if !(1..=MAX_VALUE_BYTES).contains(&length) {
                return Err(diagnostic(Code::Range, at));
            }
            let kind = if matches!(kind, DeclaredType::Utf8(_)) {
                ScalarType::Utf8
            } else {
                ScalarType::Binary
            };
            Ok((kind, Some(length)))
        }
    }
}

fn integer(
    source: &[u8],
    sign: Sign,
    digits: Span,
    span: Span,
    control: &CallControl<'_>,
) -> Result<i64, Diagnostic> {
    // Parser-owned sign/trivia has already been validated. Jump to the digits
    // without rescanning intervening comments or whitespace.
    check(control, digits.start)?;
    let mut value = 0i64;
    let mut last_check = digits.start;
    for (index, &byte) in source[digits.start..digits.end].iter().enumerate() {
        if !byte.is_ascii_digit() {
            return Err(diagnostic(Code::Syntax, span));
        }
        value = value
            .checked_mul(10)
            .and_then(|value| value.checked_sub(i64::from(byte - b'0')))
            .ok_or_else(|| diagnostic(Code::Range, span))?;
        if sign != Sign::Minus && value == i64::MIN {
            return Err(diagnostic(Code::Range, span));
        }
        let position = digits.start + index + 1;
        if position - last_check >= 256 {
            // INTEGER_CHECK: includes arbitrarily long in-range zero prefixes.
            check(control, position)?;
            last_check = position;
        }
    }
    check(control, span.end)?;
    if sign == Sign::Minus {
        Ok(value)
    } else {
        value
            .checked_neg()
            .ok_or_else(|| diagnostic(Code::Range, span))
    }
}

fn text_length(source: &[u8], span: Span, control: &CallControl<'_>) -> Result<usize, Diagnostic> {
    check(control, span.start)?;
    let mut position = span.start + 1;
    let mut last_check = span.start;
    let mut length = 0usize;
    while position < span.end - 1 {
        let (piece, next) = text_piece(source, position, span)?;
        length = length
            .checked_add(piece.len())
            .filter(|&length| length <= MAX_VALUE_BYTES)
            .ok_or_else(|| diagnostic(Code::Range, span))?;
        position = next;
        if position - last_check >= 252 {
            // TEXT_COUNT_CHECK: a following scalar can add at most four bytes.
            check(control, position)?;
            last_check = position;
        }
    }
    check(control, span.end)?;
    Ok(length)
}

fn text_piece(source: &[u8], position: usize, span: Span) -> Result<(&str, usize), Diagnostic> {
    let error = diagnostic(Code::Syntax, span);
    let first = source[position];
    let width = match first {
        b'\'' => {
            if position + 1 >= span.end - 1 || source[position + 1] != b'\'' {
                return Err(error);
            }
            return Ok(("'", position + 2));
        }
        0..=0x7f => 1,
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => return Err(error),
    };
    let end = position + width;
    if end > span.end - 1 {
        return Err(error);
    }
    // Standard UTF-8 validation sees at most one scalar, never the whole source
    // or decoded payload. Removing ASCII quotes cannot change UTF-8 boundaries.
    let piece = std::str::from_utf8(&source[position..end]).map_err(|_| error)?;
    Ok((piece, end))
}

fn binary_length(
    source: &[u8],
    span: Span,
    control: &CallControl<'_>,
) -> Result<usize, Diagnostic> {
    check(control, span.start)?;
    let mut position = span.start + 2;
    let mut last_check = span.start;
    let mut length = 0usize;
    while position < span.end - 1 {
        hex_pair(source, position, span)?;
        // Both nibbles are valid before this decoded byte exists. An invalid
        // or dangling nibble in the first excessive pair remains Syntax.
        length = length
            .checked_add(1)
            .filter(|&length| length <= MAX_VALUE_BYTES)
            .ok_or_else(|| diagnostic(Code::Range, span))?;
        position += 2;
        if position - last_check >= 256 {
            // BINARY_COUNT_CHECK: exactly two raw bytes per complete pair.
            check(control, position)?;
            last_check = position;
        }
    }
    check(control, span.end)?;
    Ok(length)
}

fn hex_pair(source: &[u8], position: usize, span: Span) -> Result<u8, Diagnostic> {
    let error = diagnostic(Code::Syntax, span);
    let high = hex_digit(source[position]).ok_or(error)?;
    if position + 1 >= span.end - 1 {
        return Err(error);
    }
    let low = hex_digit(source[position + 1]).ok_or(error)?;
    Ok((high << 4) | low)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn diagnostic(code: Code, span: Span) -> Diagnostic {
    Diagnostic {
        code,
        module: Module::Value,
        span,
    }
}

fn point(position: usize) -> Span {
    Span {
        start: position,
        end: position,
    }
}

fn check(control: &CallControl<'_>, position: usize) -> Result<(), Diagnostic> {
    control.check(Module::Value, point(position))
}

fn copy_check(control: &CallControl<'_>, position: usize, at: Span) -> Result<(), Diagnostic> {
    check(control, position).map_err(|mut error| {
        error.span = at;
        error
    })
}
