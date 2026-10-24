//! Predicate precedence and bounded nesting over the caller's existing arena.

use super::{ParseInput, check, joined, limit, syntax, unsupported};
use crate::frontend::{
    Diagnostic, Keyword, Span, Token, TokenKind,
    ast::{AstBuilder, ComparisonOp, NodeKind},
};

const MAX_PREDICATE_DEPTH: u8 = 64;

#[derive(Clone, Copy)]
enum Wrapper {
    Or,
    And,
}

struct NestedTerm {
    node: usize,
    span: Span,
    depth: u8,
    group: bool,
}

impl ParseInput<'_, '_> {
    pub(super) fn predicate_fragment(
        &mut self,
        builder: &mut AstBuilder<'_>,
    ) -> Result<(), Diagnostic> {
        let first = self.peek()?;
        // This is the only empty-input fast path. Internal Or/And wrappers are
        // registered even when their first token is EOF or a closing parenthesis.
        if first.kind == TokenKind::Eof {
            return Err(syntax(first));
        }
        let root = builder.add(NodeKind::Predicate, first.span, None, self.control)?;
        let span = self.predicate(builder, root, 0)?;
        builder.set_span(root, span);
        if self.peek()?.kind == TokenKind::Semicolon {
            self.take()?;
        }
        self.expect(TokenKind::Eof)?;
        Ok(())
    }

    // The future WHERE caller supplies its existing parent, source, arena and
    // marker counter. Only predicate_fragment adds a root or checks final EOF.
    pub(super) fn predicate(
        &mut self,
        builder: &mut AstBuilder<'_>,
        parent: usize,
        depth: u8,
    ) -> Result<Span, Diagnostic> {
        let (node, first) = self.predicate_wrapper(builder, parent, Wrapper::Or)?;
        let mut last = self.predicate_and(builder, node, depth)?;
        while self.peek()?.kind == TokenKind::Keyword(Keyword::Or) {
            let operator = self.take()?;
            check(self.control, operator.span.end)?;
            last = self.predicate_and(builder, node, depth)?;
        }
        self.finish_predicate_node(builder, node, first, last)
    }

    fn predicate_and(
        &mut self,
        builder: &mut AstBuilder<'_>,
        parent: usize,
        depth: u8,
    ) -> Result<Span, Diagnostic> {
        let (node, first) = self.predicate_wrapper(builder, parent, Wrapper::And)?;
        let mut last = self.predicate_term(builder, node, depth)?;
        while self.peek()?.kind == TokenKind::Keyword(Keyword::And) {
            let operator = self.take()?;
            check(self.control, operator.span.end)?;
            last = self.predicate_term(builder, node, depth)?;
        }
        self.finish_predicate_node(builder, node, first, last)
    }

    fn predicate_term(
        &mut self,
        builder: &mut AstBuilder<'_>,
        parent: usize,
        depth: u8,
    ) -> Result<Span, Diagnostic> {
        let Some(nested) = self.start_predicate_term(builder, parent, depth)? else {
            return self.predicate_atom(builder, parent);
        };
        let last = if nested.group {
            self.predicate(builder, nested.node, nested.depth)?;
            self.expect(TokenKind::RightParen)?.span
        } else {
            self.predicate_term(builder, nested.node, nested.depth)?
        };
        // This also observes every closing parenthesis during an unwind that
        // consumes no new nodes. The recursive frame holds only small metadata.
        self.finish_predicate_node(builder, nested.node, nested.span, last)
    }

    // Keep large NodeKind temporaries and setup/error branches outside frames
    // retained during recursive descent, including in unoptimized builds.
    #[inline(never)]
    fn predicate_wrapper(
        &mut self,
        builder: &mut AstBuilder<'_>,
        parent: usize,
        wrapper: Wrapper,
    ) -> Result<(usize, Span), Diagnostic> {
        let first = self.peek()?;
        let kind = match wrapper {
            Wrapper::Or => NodeKind::Or,
            Wrapper::And => NodeKind::And,
        };
        let node = builder.add(kind, first.span, Some(parent), self.control)?;
        Ok((node, first.span))
    }

    #[inline(never)]
    fn start_predicate_term(
        &mut self,
        builder: &mut AstBuilder<'_>,
        parent: usize,
        depth: u8,
    ) -> Result<Option<NestedTerm>, Diagnostic> {
        let first = self.peek()?;
        check(self.control, first.span.start)?;
        let group = match first.kind {
            TokenKind::Keyword(Keyword::Not) => false,
            TokenKind::LeftParen => true,
            _ => return Ok(None),
        };
        let next_depth = nested_depth(depth, first)?;
        self.take()?;
        let kind = if group {
            NodeKind::Group
        } else {
            NodeKind::Not
        };
        let node = builder.add(kind, first.span, Some(parent), self.control)?;
        Ok(Some(NestedTerm {
            node,
            span: first.span,
            depth: next_depth,
            group,
        }))
    }

    #[inline(never)]
    fn finish_predicate_node(
        &self,
        builder: &mut AstBuilder<'_>,
        node: usize,
        first: Span,
        last: Span,
    ) -> Result<Span, Diagnostic> {
        let span = joined(first, last);
        check(self.control, span.end)?;
        builder.set_span(node, span);
        Ok(span)
    }

    fn predicate_atom(
        &mut self,
        builder: &mut AstBuilder<'_>,
        parent: usize,
    ) -> Result<Span, Diagnostic> {
        let first = self.peek()?;
        // Parent-before-child construction keeps existing name/scalar helpers.
        // The provisional kind is private and never escapes an incomplete parse.
        let node = builder.add(
            NodeKind::Compare(ComparisonOp::Eq),
            first.span,
            Some(parent),
            self.control,
        )?;
        let left = self.predicate_operand(builder, node)?;
        let operator = self.take()?;
        let last = if let Some(comparison) = comparison(operator.kind) {
            builder.set_kind(node, NodeKind::Compare(comparison));
            self.predicate_operand(builder, node)?
        } else if operator.kind == TokenKind::Keyword(Keyword::Is) {
            let mut null = self.take()?;
            let negated = null.kind == TokenKind::Keyword(Keyword::Not);
            if negated {
                null = self.take()?;
            }
            if null.kind != TokenKind::Keyword(Keyword::Null) {
                return Err(
                    if matches!(
                        null.kind,
                        TokenKind::Keyword(Keyword::True | Keyword::False)
                    ) || self.word(null, b"DISTINCT")
                    {
                        unsupported(null)
                    } else {
                        syntax(null)
                    },
                );
            }
            builder.set_kind(node, NodeKind::IsNull { negated });
            null.span
        } else {
            if self.predicate_future_operator(operator) {
                return Err(unsupported(operator));
            }
            if operator.kind == TokenKind::Keyword(Keyword::Not) {
                let next = self.peek()?;
                if self.predicate_future_operator(next) {
                    return Err(unsupported(operator));
                }
            }
            return Err(syntax(operator));
        };
        let span = joined(left, last);
        check(self.control, span.end)?;
        builder.set_span(node, span);
        Ok(span)
    }

    fn predicate_operand(
        &mut self,
        builder: &mut AstBuilder<'_>,
        parent: usize,
    ) -> Result<Span, Diagnostic> {
        let first = self.peek()?;
        let span = match first.kind {
            TokenKind::Identifier | TokenKind::QuotedIdentifier => {
                if self.prefixed_text(first) {
                    return Err(unsupported(first));
                }
                self.name(builder, parent)?
            }
            TokenKind::LeftParen => {
                // Scalar parentheses are outside the grammar. This bounded
                // lookahead distinguishes the explicitly unsupported subquery.
                self.take()?;
                let next = self.peek()?;
                return Err(if next.kind == TokenKind::Keyword(Keyword::Select) {
                    unsupported(next)
                } else {
                    syntax(first)
                });
            }
            _ => self.scalar(builder, parent)?,
        };
        let suffix = self.peek()?;
        if matches!(
            suffix.kind,
            TokenKind::Dot
                | TokenKind::LeftParen
                | TokenKind::Plus
                | TokenKind::Minus
                | TokenKind::Star
        ) {
            return Err(unsupported(suffix));
        }
        Ok(span)
    }

    fn predicate_future_operator(&self, token: Token) -> bool {
        self.word(token, b"IN") || self.word(token, b"BETWEEN") || self.word(token, b"LIKE")
    }
}

fn nested_depth(depth: u8, token: Token) -> Result<u8, Diagnostic> {
    if depth == MAX_PREDICATE_DEPTH {
        Err(limit(token.span.start))
    } else {
        Ok(depth + 1)
    }
}

fn comparison(kind: TokenKind) -> Option<ComparisonOp> {
    match kind {
        TokenKind::Equal => Some(ComparisonOp::Eq),
        TokenKind::NotEqual => Some(ComparisonOp::NotEq),
        TokenKind::Less => Some(ComparisonOp::Lt),
        TokenKind::LessEqual => Some(ComparisonOp::Le),
        TokenKind::Greater => Some(ComparisonOp::Gt),
        TokenKind::GreaterEqual => Some(ComparisonOp::Ge),
        _ => None,
    }
}
