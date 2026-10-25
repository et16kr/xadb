//! Owned, bounded statement and predicate syntax without value or DB effects.
//!
//! Literal payloads remain raw source spans. The later scalar input owner must
//! validate integer ranges, binary hex, decoded lengths, and typed empty text.

use super::{
    CallControl, Code, Diagnostic, Keyword, Lexer, Module, Span, Token, TokenKind,
    ast::{AstBuilder, NodeKind, ParsedStatement, ScalarSyntax, Sign},
    name::normalize_name,
    parse_budget::ParseBudget,
};
use foundation::runtime::Quota;

mod ddl;
mod predicate;
mod statement;

const MAX_MARKERS: u16 = 256;

/// Reusable parser charging owned outputs to the caller's shared SQL quotas.
/// Each call supplies fresh controls; returned statements keep their own leases
/// and remain valid after the caller's input or this parser is dropped.
pub struct Parser {
    budget: ParseBudget,
}

impl Parser {
    /// Use the existing session and database quota ledgers for every call.
    pub fn new(session: &Quota, database: &Quota) -> Self {
        Self {
            budget: ParseBudget::new(session, database),
        }
    }

    /// Parse exactly one supported statement into an owned flat AST.
    /// A final semicolon and trailing trivia are optional. Successful parsing
    /// preserves raw scalar syntax and does not certify values or execute SQL.
    ///
    /// # Errors
    /// Returns lexical/name diagnostics unchanged, or a Parse diagnostic for
    /// syntax, explicitly unsupported forms, quotas, limits, or call controls.
    /// An error releases this call's partial output and permits parser reuse.
    pub fn parse(
        &mut self,
        source: &[u8],
        control: &CallControl<'_>,
    ) -> Result<ParsedStatement, Diagnostic> {
        // Even a cancelled or malformed-input call consumes its one-call fault
        // setting. This bookkeeping allocates nothing and observes no controls.
        self.budget.begin_call();
        check(control, 0)?;
        let lexer = Lexer::new(source, control)?;
        let mut builder = AstBuilder::new(source, &mut self.budget, control)?;
        let mut input = ParseInput {
            source,
            control,
            lexer,
            lookahead: None,
            marker_count: 0,
        };
        input.statement(&mut builder)?;
        check(control, source.len())?;
        Ok(builder.finish(input.marker_count))
    }

    /// Parse one predicate fragment without a WHERE keyword or catalog access.
    /// A final semicolon and trailing trivia are optional. The returned raw AST
    /// preserves names and scalar syntax without validating types or values.
    ///
    /// # Errors
    /// Returns lexical/name diagnostics unchanged, or a Parse diagnostic for
    /// syntax, unsupported forms, limits, quota exhaustion, or call controls.
    /// Failed calls release their partial output and leave this parser reusable.
    pub fn parse_predicate(
        &mut self,
        source: &[u8],
        control: &CallControl<'_>,
    ) -> Result<ParsedStatement, Diagnostic> {
        self.budget.begin_call();
        check(control, 0)?;
        let lexer = Lexer::new(source, control)?;
        let mut builder = AstBuilder::new(source, &mut self.budget, control)?;
        let mut input = ParseInput {
            source,
            control,
            lexer,
            lookahead: None,
            marker_count: 0,
        };
        input.predicate_fragment(&mut builder)?;
        check(control, source.len())?;
        Ok(builder.finish(input.marker_count))
    }

    /// Fail a backing allocation in the next call, numbered from one.
    /// Zero disables injection; an unreached ordinal never carries forward.
    #[cfg(feature = "test-support")]
    pub fn fail_allocation_at(&mut self, ordinal: usize) {
        self.budget.fail_allocation_at(ordinal);
    }

    /// Add elements to backing requests to exercise actual-capacity accounting.
    /// The padding persists across calls until explicitly set to zero.
    #[cfg(feature = "test-support")]
    pub fn set_allocation_padding(&mut self, elements: usize) {
        self.budget.set_allocation_padding(elements);
    }

    /// Observe reached backing allocation sites in the current or previous call.
    #[cfg(feature = "test-support")]
    pub fn allocation_count(&self) -> usize {
        self.budget.allocation_count()
    }
}

// Only this call borrows the input/control. The builder independently owns the
// copied bytes and all nodes, so its final output has no self-reference.
struct ParseInput<'source, 'call> {
    source: &'source [u8],
    control: &'call CallControl<'call>,
    lexer: Lexer<'source, 'call>,
    lookahead: Option<Token>,
    marker_count: u16,
}

impl ParseInput<'_, '_> {
    fn statement(&mut self, builder: &mut AstBuilder<'_>) -> Result<(), Diagnostic> {
        let first = self.take()?;
        let is_insert = match first.kind {
            TokenKind::Keyword(Keyword::Insert) => {
                self.insert(builder, first)?;
                true
            }
            TokenKind::Keyword(Keyword::Begin) => {
                builder.add(NodeKind::Begin, first.span, None, self.control)?;
                false
            }
            TokenKind::Keyword(Keyword::Commit) => {
                builder.add(NodeKind::Commit, first.span, None, self.control)?;
                false
            }
            TokenKind::Keyword(Keyword::Rollback) => {
                builder.add(NodeKind::Rollback, first.span, None, self.control)?;
                false
            }
            TokenKind::Keyword(Keyword::Set) => {
                self.autocommit(builder, first)?;
                false
            }
            TokenKind::Keyword(Keyword::Select) => {
                self.select_statement(builder, first)?;
                false
            }
            TokenKind::Keyword(Keyword::Update) => {
                self.update_statement(builder, first)?;
                false
            }
            TokenKind::Keyword(Keyword::Delete) => {
                self.delete_statement(builder, first)?;
                false
            }
            TokenKind::Keyword(Keyword::Create) => {
                self.create_statement(builder, first)?;
                false
            }
            TokenKind::Keyword(Keyword::Drop) => {
                self.drop_statement(builder, first)?;
                false
            }
            TokenKind::Identifier
                if self.word(first, b"PREPARE")
                    || self.word(first, b"EXECUTE")
                    || self.word(first, b"WITH")
                    || self.word(first, b"ALTER")
                    || self.word(first, b"SAVEPOINT")
                    || self.word(first, b"EXPLAIN") =>
            {
                return Err(unsupported(first));
            }
            _ => return Err(syntax(first)),
        };

        let trailing = self.peek()?;
        if is_insert && self.word(trailing, b"RETURNING") {
            return Err(unsupported(trailing));
        }
        if trailing.kind == TokenKind::Semicolon {
            self.take()?;
        }
        self.expect(TokenKind::Eof)?;
        Ok(())
    }

    fn autocommit(&mut self, builder: &mut AstBuilder<'_>, first: Token) -> Result<(), Diagnostic> {
        self.expect(TokenKind::Keyword(Keyword::Autocommit))?;
        let last = self.take()?;
        let enabled = match last.kind {
            TokenKind::Keyword(Keyword::On) => true,
            TokenKind::Keyword(Keyword::Off) => false,
            _ => return Err(syntax(last)),
        };
        let root = builder.add(
            NodeKind::SetAutocommit(enabled),
            first.span,
            None,
            self.control,
        )?;
        builder.set_span(root, joined(first.span, last.span));
        Ok(())
    }

    fn insert(&mut self, builder: &mut AstBuilder<'_>, first: Token) -> Result<(), Diagnostic> {
        let root = builder.add(NodeKind::Insert, first.span, None, self.control)?;
        self.expect(TokenKind::Keyword(Keyword::Into))?;
        self.target(builder, root)?;
        self.reject_insert_extension(true)?;
        self.columns(builder, root)?;
        self.reject_insert_extension(false)?;
        let values = self.expect(TokenKind::Keyword(Keyword::Values))?;
        let rows = builder.add(NodeKind::Rows, values.span, Some(root), self.control)?;
        let last = loop {
            let end = self.tuple(builder, rows)?;
            if self.peek()?.kind != TokenKind::Comma {
                break end;
            }
            self.take()?;
        };
        builder.set_span(rows, joined(values.span, last));
        builder.set_span(root, joined(first.span, last));
        Ok(())
    }

    fn target(&mut self, builder: &mut AstBuilder<'_>, root: usize) -> Result<Span, Diagnostic> {
        let first = self.peek_name()?;
        let target = builder.add(NodeKind::Target, first.span, Some(root), self.control)?;
        let name = normalize_name(self.source, self.take()?, self.control)?;
        let is_public = name.as_bytes() == b"PUBLIC";
        builder.add(NodeKind::Name(name), first.span, Some(target), self.control)?;
        let last = if self.peek()?.kind == TokenKind::Dot {
            if !is_public {
                return Err(unsupported(first));
            }
            self.take()?;
            self.name(builder, target)?
        } else {
            first.span
        };
        let span = joined(first.span, last);
        builder.set_span(target, span);
        Ok(span)
    }

    fn columns(&mut self, builder: &mut AstBuilder<'_>, root: usize) -> Result<(), Diagnostic> {
        let open = self.expect(TokenKind::LeftParen)?;
        let columns = builder.add(NodeKind::Columns, open.span, Some(root), self.control)?;
        self.name(builder, columns)?;
        while self.peek()?.kind == TokenKind::Comma {
            self.take()?;
            self.name(builder, columns)?;
        }
        let close = self.expect(TokenKind::RightParen)?;
        builder.set_span(columns, joined(open.span, close.span));
        Ok(())
    }

    fn tuple(&mut self, builder: &mut AstBuilder<'_>, rows: usize) -> Result<Span, Diagnostic> {
        let open = self.expect(TokenKind::LeftParen)?;
        let tuple = builder.add(NodeKind::Tuple, open.span, Some(rows), self.control)?;
        self.scalar(builder, tuple)?;
        while self.peek()?.kind == TokenKind::Comma {
            self.take()?;
            self.scalar(builder, tuple)?;
        }
        let close = self.expect(TokenKind::RightParen)?;
        builder.set_span(tuple, joined(open.span, close.span));
        Ok(close.span)
    }

    fn name(&mut self, builder: &mut AstBuilder<'_>, parent: usize) -> Result<Span, Diagnostic> {
        let token = self.peek_name()?;
        self.take()?;
        let name = normalize_name(self.source, token, self.control)?;
        builder.add(NodeKind::Name(name), token.span, Some(parent), self.control)?;
        Ok(token.span)
    }

    fn scalar(&mut self, builder: &mut AstBuilder<'_>, tuple: usize) -> Result<Span, Diagnostic> {
        let first = self.take()?;
        let (kind, span) = match first.kind {
            TokenKind::Digits => self.integer(first, first, Sign::None)?,
            TokenKind::Plus | TokenKind::Minus => {
                let digits = self.take()?;
                if digits.kind != TokenKind::Digits {
                    return Err(self.expected_scalar(digits));
                }
                let sign = if first.kind == TokenKind::Plus {
                    Sign::Plus
                } else {
                    Sign::Minus
                };
                self.integer(first, digits, sign)?
            }
            TokenKind::Text => (ScalarSyntax::Text, first.span),
            TokenKind::Binary => (ScalarSyntax::Binary, first.span),
            TokenKind::Keyword(Keyword::True) => (ScalarSyntax::Bool(true), first.span),
            TokenKind::Keyword(Keyword::False) => (ScalarSyntax::Bool(false), first.span),
            TokenKind::Keyword(Keyword::Null) => (ScalarSyntax::Null, first.span),
            TokenKind::Marker => {
                if self.marker_count == MAX_MARKERS {
                    return Err(limit(first.span.start));
                }
                self.marker_count += 1;
                (
                    ScalarSyntax::Parameter {
                        ordinal: self.marker_count,
                    },
                    first.span,
                )
            }
            TokenKind::Keyword(Keyword::Select) => return Err(unsupported(first)),
            TokenKind::Identifier if self.word(first, b"DEFAULT") || self.prefixed_text(first) => {
                return Err(unsupported(first));
            }
            _ => return Err(self.expected_scalar(first)),
        };
        builder.add(NodeKind::Scalar(kind), span, Some(tuple), self.control)?;
        Ok(span)
    }

    fn integer(
        &mut self,
        first: Token,
        digits: Token,
        sign: Sign,
    ) -> Result<(ScalarSyntax, Span), Diagnostic> {
        let next = self.peek()?;
        if digits.span.end == next.span.start
            && (next.kind == TokenKind::Dot || self.exponent(next)?)
        {
            return Err(unsupported(next));
        }
        Ok((
            ScalarSyntax::Integer {
                sign,
                digits: digits.span,
            },
            joined(first.span, digits.span),
        ))
    }

    fn exponent(&self, token: Token) -> Result<bool, Diagnostic> {
        if token.kind != TokenKind::Identifier {
            return Ok(false);
        }
        let raw = &self.source[token.span.start..token.span.end];
        if !matches!(raw.first(), Some(b'e' | b'E')) {
            return Ok(false);
        }
        let mut position = token.span.start + 1;
        check(self.control, position)?;
        for chunk in raw[1..].chunks(256) {
            if !chunk.iter().all(u8::is_ascii_digit) {
                return Ok(false);
            }
            position += chunk.len();
            check(self.control, position)?;
        }
        Ok(true)
    }

    fn reject_insert_extension(&mut self, after_target: bool) -> Result<(), Diagnostic> {
        let token = self.peek()?;
        if token.kind == TokenKind::Keyword(Keyword::Select)
            || self.word(token, b"DEFAULT")
            || (after_target && token.kind == TokenKind::Keyword(Keyword::As))
        {
            Err(unsupported(token))
        } else {
            Ok(())
        }
    }

    fn expected_scalar(&self, token: Token) -> Diagnostic {
        if token.kind == TokenKind::Dot
            && self
                .source
                .get(token.span.end)
                .is_some_and(u8::is_ascii_digit)
        {
            unsupported(token)
        } else {
            syntax(token)
        }
    }

    fn prefixed_text(&self, token: Token) -> bool {
        token.span.end - token.span.start == 1
            && matches!(self.source[token.span.start], b'n' | b'N' | b'q' | b'Q')
            && self.source.get(token.span.end) == Some(&b'\'')
    }

    fn word(&self, token: Token, expected: &[u8]) -> bool {
        token.kind == TokenKind::Identifier
            && self.source[token.span.start..token.span.end].eq_ignore_ascii_case(expected)
    }

    fn peek_name(&mut self) -> Result<Token, Diagnostic> {
        let token = self.peek()?;
        if matches!(
            token.kind,
            TokenKind::Identifier | TokenKind::QuotedIdentifier
        ) {
            Ok(token)
        } else {
            Err(syntax(token))
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Result<Token, Diagnostic> {
        let token = self.take()?;
        if token.kind == kind {
            Ok(token)
        } else {
            Err(syntax(token))
        }
    }

    fn peek(&mut self) -> Result<Token, Diagnostic> {
        if let Some(token) = self.lookahead {
            return Ok(token);
        }
        let token = self.lexer.next_token()?;
        self.lookahead = Some(token);
        Ok(token)
    }

    fn take(&mut self) -> Result<Token, Diagnostic> {
        let token = self.peek()?;
        self.lookahead = None;
        Ok(token)
    }
}

fn joined(first: Span, last: Span) -> Span {
    Span {
        start: first.start,
        end: last.end,
    }
}

fn syntax(token: Token) -> Diagnostic {
    Diagnostic {
        code: Code::Syntax,
        module: Module::Parse,
        span: token.span,
    }
}

fn unsupported(token: Token) -> Diagnostic {
    Diagnostic {
        code: Code::Unsupported,
        module: Module::Parse,
        span: token.span,
    }
}

fn limit(position: usize) -> Diagnostic {
    Diagnostic {
        code: Code::Limit,
        module: Module::Parse,
        span: Span {
            start: position,
            end: position,
        },
    }
}

fn check(control: &CallControl<'_>, position: usize) -> Result<(), Diagnostic> {
    control.check(
        Module::Parse,
        Span {
            start: position,
            end: position,
        },
    )
}
