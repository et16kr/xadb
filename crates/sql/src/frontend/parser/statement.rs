//! Raw query and DML clauses over the existing source, arena and marker counter.

use super::{ParseInput, check, joined, limit, syntax, unsupported};
use crate::frontend::{
    Diagnostic, Keyword, NullPlacement, SortDirection, Span, Token, TokenKind,
    ast::{AstBuilder, NodeKind},
    name::normalize_name,
};

const MAX_LIST_ITEMS: usize = 32;

impl ParseInput<'_, '_> {
    pub(super) fn select_statement(
        &mut self,
        builder: &mut AstBuilder<'_>,
        first: Token,
    ) -> Result<(), Diagnostic> {
        let root = builder.add(NodeKind::Select, first.span, None, self.control)?;
        self.projection(builder, root)?;
        self.expect(TokenKind::Keyword(Keyword::From))?;
        let mut last = self.statement_target(builder, root)?;
        self.reject_target_alias()?;
        if self.peek()?.kind == TokenKind::Comma {
            return Err(unsupported(self.peek()?));
        }
        if self.peek()?.kind == TokenKind::Keyword(Keyword::Where) {
            last = self.where_clause(builder, root)?;
        }
        if self.peek()?.kind == TokenKind::Keyword(Keyword::Order) {
            last = self.order_clause(builder, root)?;
        }
        if self.peek()?.kind == TokenKind::Keyword(Keyword::Limit) {
            last = self.limit_clause(builder, root)?;
        }
        let trailing = self.peek()?;
        if [
            b"GROUP".as_slice(),
            b"HAVING",
            b"WINDOW",
            b"UNION",
            b"INTERSECT",
            b"EXCEPT",
            b"FETCH",
            b"OFFSET",
            b"FOR",
        ]
        .iter()
        .any(|word| self.word(trailing, word))
        {
            return Err(unsupported(trailing));
        }
        self.complete_statement_node(builder, root, first.span, last)?;
        Ok(())
    }

    pub(super) fn update_statement(
        &mut self,
        builder: &mut AstBuilder<'_>,
        first: Token,
    ) -> Result<(), Diagnostic> {
        let root = builder.add(NodeKind::Update, first.span, None, self.control)?;
        self.statement_target(builder, root)?;
        self.reject_target_alias()?;
        let mut last = self.assignments(builder, root)?;
        if self.peek()?.kind == TokenKind::Keyword(Keyword::Where) {
            last = self.where_clause(builder, root)?;
        }
        self.reject_dml_extension(true)?;
        self.complete_statement_node(builder, root, first.span, last)?;
        Ok(())
    }

    pub(super) fn delete_statement(
        &mut self,
        builder: &mut AstBuilder<'_>,
        first: Token,
    ) -> Result<(), Diagnostic> {
        let root = builder.add(NodeKind::Delete, first.span, None, self.control)?;
        self.expect(TokenKind::Keyword(Keyword::From))?;
        let mut last = self.statement_target(builder, root)?;
        self.reject_target_alias()?;
        if self.peek()?.kind == TokenKind::Keyword(Keyword::Where) {
            last = self.where_clause(builder, root)?;
        }
        self.reject_dml_extension(false)?;
        self.complete_statement_node(builder, root, first.span, last)?;
        Ok(())
    }

    fn statement_target(
        &mut self,
        builder: &mut AstBuilder<'_>,
        root: usize,
    ) -> Result<Span, Diagnostic> {
        let span = self.target(builder, root)?;
        let suffix = self.peek()?;
        if suffix.kind == TokenKind::Dot {
            return Err(unsupported(suffix));
        }
        check(self.control, span.end)?;
        Ok(span)
    }

    fn reject_target_alias(&mut self) -> Result<(), Diagnostic> {
        let token = self.peek()?;
        if name_kind(token.kind) || token.kind == TokenKind::Keyword(Keyword::As) {
            Err(unsupported(token))
        } else {
            Ok(())
        }
    }

    fn projection(
        &mut self,
        builder: &mut AstBuilder<'_>,
        root: usize,
    ) -> Result<Span, Diagnostic> {
        let first = self.peek()?;
        let projection = builder.add(NodeKind::Projection, first.span, Some(root), self.control)?;
        if first.kind == TokenKind::Star {
            check(self.control, first.span.start)?;
            self.take()?;
            builder.add(
                NodeKind::Wildcard,
                first.span,
                Some(projection),
                self.control,
            )?;
            let suffix = self.peek()?;
            if suffix.kind == TokenKind::Comma || expression_suffix(suffix.kind) {
                return Err(unsupported(suffix));
            }
            return self.complete_statement_node(builder, projection, first.span, first.span);
        }
        let mut count = 0;
        let last = loop {
            let token = self.peek()?;
            check(self.control, token.span.start)?;
            // Only this first unquoted name can introduce a modifier. Preserve
            // the consumed token when lookahead instead proves an ordinary name.
            let consumed =
                count == 0 && (self.word(token, b"DISTINCT") || self.word(token, b"ALL"));
            if consumed {
                self.take()?;
                let next = self.peek()?;
                if name_kind(next.kind) || expression_start(next.kind) {
                    return Err(unsupported(token));
                }
            }
            if expression_start(token.kind) {
                return Err(unsupported(token));
            }
            if !name_kind(token.kind) {
                return Err(syntax(token));
            }
            if count == MAX_LIST_ITEMS {
                return Err(limit(token.span.start));
            }
            let item = builder.add(
                NodeKind::ProjectionItem,
                token.span,
                Some(projection),
                self.control,
            )?;
            if !consumed {
                self.take()?;
            }
            let name = normalize_name(self.source, token, self.control)?;
            builder.add(NodeKind::Name(name), token.span, Some(item), self.control)?;
            let mut last = token.span;
            self.reject_expression_suffix()?;
            if self.peek()?.kind == TokenKind::Keyword(Keyword::As) {
                self.take()?;
                last = self.name(builder, item)?;
                self.reject_expression_suffix()?;
            }
            let next = self.peek()?;
            if name_kind(next.kind) {
                return Err(unsupported(next));
            }
            self.complete_statement_node(builder, item, token.span, last)?;
            count += 1;
            if next.kind != TokenKind::Comma {
                break last;
            }
            let comma = self.take()?;
            check(self.control, comma.span.end)?;
        };
        self.complete_statement_node(builder, projection, first.span, last)
    }

    fn where_clause(
        &mut self,
        builder: &mut AstBuilder<'_>,
        root: usize,
    ) -> Result<Span, Diagnostic> {
        let first = self.expect(TokenKind::Keyword(Keyword::Where))?;
        let node = builder.add(NodeKind::Where, first.span, Some(root), self.control)?;
        // Reuse the caller's marker counter; fixed wrappers consume no depth.
        let last = self.predicate(builder, node, 0)?;
        self.complete_statement_node(builder, node, first.span, last)
    }

    fn order_clause(
        &mut self,
        builder: &mut AstBuilder<'_>,
        root: usize,
    ) -> Result<Span, Diagnostic> {
        let first = self.expect(TokenKind::Keyword(Keyword::Order))?;
        let node = builder.add(NodeKind::OrderBy, first.span, Some(root), self.control)?;
        self.expect(TokenKind::Keyword(Keyword::By))?;
        let mut count = 0;
        let last = loop {
            let token = self.peek()?;
            check(self.control, token.span.start)?;
            if expression_start(token.kind) {
                return Err(unsupported(token));
            }
            if !name_kind(token.kind) {
                return Err(syntax(token));
            }
            if count == MAX_LIST_ITEMS {
                return Err(limit(token.span.start));
            }
            let item = builder.add(
                NodeKind::OrderTerm {
                    direction: None,
                    nulls: None,
                },
                token.span,
                Some(node),
                self.control,
            )?;
            let mut last = self.name(builder, item)?;
            self.reject_expression_suffix()?;
            let direction = match self.peek()?.kind {
                TokenKind::Keyword(Keyword::Asc) => {
                    last = self.take()?.span;
                    Some(SortDirection::Asc)
                }
                TokenKind::Keyword(Keyword::Desc) => {
                    last = self.take()?.span;
                    Some(SortDirection::Desc)
                }
                _ => None,
            };
            let nulls = if self.peek()?.kind == TokenKind::Keyword(Keyword::Nulls) {
                self.take()?;
                let placement = self.take()?;
                last = placement.span;
                Some(match placement.kind {
                    TokenKind::Keyword(Keyword::First) => NullPlacement::First,
                    TokenKind::Keyword(Keyword::Last) => NullPlacement::Last,
                    _ => return Err(syntax(placement)),
                })
            } else {
                None
            };
            let next = self.peek()?;
            self.complete_statement_node(builder, item, token.span, last)?;
            builder.set_kind(item, NodeKind::OrderTerm { direction, nulls });
            count += 1;
            if next.kind != TokenKind::Comma {
                break last;
            }
            let comma = self.take()?;
            check(self.control, comma.span.end)?;
        };
        self.complete_statement_node(builder, node, first.span, last)
    }

    fn limit_clause(
        &mut self,
        builder: &mut AstBuilder<'_>,
        root: usize,
    ) -> Result<Span, Diagnostic> {
        let first = self.expect(TokenKind::Keyword(Keyword::Limit))?;
        let node = builder.add(NodeKind::Limit, first.span, Some(root), self.control)?;
        let value = self.peek()?;
        if !matches!(value.kind, TokenKind::Digits | TokenKind::Marker) {
            return Err(self.expected_scalar(value));
        }
        let last = self.scalar(builder, node)?;
        self.reject_expression_suffix()?;
        if self.peek()?.kind == TokenKind::Comma {
            return Err(unsupported(self.peek()?));
        }
        self.complete_statement_node(builder, node, first.span, last)
    }

    fn assignments(
        &mut self,
        builder: &mut AstBuilder<'_>,
        root: usize,
    ) -> Result<Span, Diagnostic> {
        let first = self.expect(TokenKind::Keyword(Keyword::Set))?;
        let node = builder.add(NodeKind::Assignments, first.span, Some(root), self.control)?;
        let last = loop {
            let token = self.peek()?;
            check(self.control, token.span.start)?;
            self.peek_name()?;
            let item = builder.add(NodeKind::Assignment, token.span, Some(node), self.control)?;
            self.name(builder, item)?;
            self.reject_expression_suffix()?;
            self.expect(TokenKind::Equal)?;
            let value = self.peek()?;
            if name_kind(value.kind) {
                return Err(unsupported(value));
            }
            if value.kind == TokenKind::LeftParen {
                self.take()?;
                let next = self.peek()?;
                return Err(if next.kind == TokenKind::Keyword(Keyword::Select) {
                    unsupported(next)
                } else {
                    syntax(value)
                });
            }
            let last = self.scalar(builder, item)?;
            self.reject_expression_suffix()?;
            let next = self.peek()?;
            self.complete_statement_node(builder, item, token.span, last)?;
            if next.kind != TokenKind::Comma {
                break last;
            }
            let comma = self.take()?;
            check(self.control, comma.span.end)?;
        };
        self.complete_statement_node(builder, node, first.span, last)
    }

    fn reject_expression_suffix(&mut self) -> Result<(), Diagnostic> {
        let token = self.peek()?;
        if expression_suffix(token.kind) {
            Err(unsupported(token))
        } else {
            Ok(())
        }
    }

    fn reject_dml_extension(&mut self, update: bool) -> Result<(), Diagnostic> {
        let token = self.peek()?;
        if self.word(token, b"RETURNING")
            || (!update && self.word(token, b"USING"))
            || (update && token.kind == TokenKind::Keyword(Keyword::From))
            || matches!(
                token.kind,
                TokenKind::Keyword(Keyword::Order | Keyword::Limit)
            )
        {
            Err(unsupported(token))
        } else {
            Ok(())
        }
    }

    fn complete_statement_node(
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
}

fn name_kind(kind: TokenKind) -> bool {
    matches!(kind, TokenKind::Identifier | TokenKind::QuotedIdentifier)
}

fn expression_start(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Digits
            | TokenKind::Text
            | TokenKind::Binary
            | TokenKind::Keyword(Keyword::True | Keyword::False | Keyword::Null)
            | TokenKind::Marker
            | TokenKind::Plus
            | TokenKind::Minus
            | TokenKind::LeftParen
            | TokenKind::Star
    )
}

fn expression_suffix(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Dot
            | TokenKind::LeftParen
            | TokenKind::Plus
            | TokenKind::Minus
            | TokenKind::Star
    )
}
