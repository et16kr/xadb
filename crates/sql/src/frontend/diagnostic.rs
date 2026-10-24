//! Allocation-free causes and original byte positions; no session outcome claims.

/// Original UTF-8 byte interval `[start,end)`, including zero-width checkpoints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    /// Inclusive byte offset.
    pub start: usize,
    /// Exclusive byte offset.
    pub end: usize,
}

/// Failure cause owned by the input boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Code {
    /// Invalid encoding, lexical structure, or raw grammar.
    Syntax,
    /// Recognized syntax outside the selected SQL grammar.
    Unsupported,
    /// Invalid identifier syntax, control character, or normalized byte length.
    Name,
    /// Declared capacity, shared quota, allocation, or offset range exhausted.
    Limit,
    /// The current call's cancellation was observed.
    Cancelled,
    /// The current call's monotonic deadline was reached.
    Timeout,
}

/// Where the cause was detected, independent of an eventual SQL execution phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Module {
    /// Source validation and token scanning.
    Lex,
    /// Identifier validation and normalization.
    Name,
    /// Streaming boundary recognition and owned source allocation.
    Split,
    /// Raw grammar, owned AST, and parser backing allocation.
    Parse,
}

/// Small failure result that does not allocate or manufacture transaction effects.
/// An eventual SQL adapter owns phase, cleanup, certainty and transaction outcomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// Cause category.
    pub code: Code,
    /// Detecting frontend component.
    pub module: Module,
    /// Original byte range; an invalid caller name range is clamped to the source.
    pub span: Span,
}
