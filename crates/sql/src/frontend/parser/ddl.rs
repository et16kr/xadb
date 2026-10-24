//! Raw DDL syntax over the existing source and flat, quota-accounted arena.

use super::{ParseInput, check, joined, syntax, unsupported};
use crate::frontend::{
    Diagnostic, Keyword, NullabilitySyntax, Sign, Span, Token, TokenKind, TypeLengthSyntax,
    TypeSyntax,
    ast::{AstBuilder, NodeKind},
    name::normalize_name,
};

#[derive(Clone, Copy)]
enum ConditionalTarget {
    None,
    Create,
    Drop,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DefinitionPhase {
    Columns,
    PrimaryKey,
    Unique,
}

impl ParseInput<'_, '_> {
    pub(super) fn create_statement(
        &mut self,
        builder: &mut AstBuilder<'_>,
        first: Token,
    ) -> Result<(), Diagnostic> {
        // Admit the root before inspecting the second token, including Lex errors.
        let root = builder.add(NodeKind::CreateTable, first.span, None, self.control)?;
        let object = self.take()?;
        let last = match object.kind {
            TokenKind::Keyword(Keyword::Table) => {
                self.ddl_target(builder, root, ConditionalTarget::Create)?;
                self.ddl_table_definition(builder, root)?;
                self.ddl_storage(builder, root)?
            }
            TokenKind::Keyword(Keyword::Index | Keyword::Unique) => {
                let unique = object.kind == TokenKind::Keyword(Keyword::Unique);
                if unique {
                    self.expect(TokenKind::Keyword(Keyword::Index))?;
                }
                builder.set_kind(root, NodeKind::CreateIndex { unique });
                self.ddl_target(builder, root, ConditionalTarget::Create)?;
                self.expect(TokenKind::Keyword(Keyword::On))?;
                self.ddl_target(builder, root, ConditionalTarget::None)?;
                let last = self.ddl_key_columns(builder, root, true)?;
                let next = self.peek()?;
                if next.kind == TokenKind::Keyword(Keyword::Where)
                    || self.word(next, b"INCLUDE")
                    || self.word(next, b"USING")
                {
                    return Err(unsupported(next));
                }
                last
            }
            _ => {
                if [
                    b"TEMP".as_slice(),
                    b"TEMPORARY",
                    b"VIEW",
                    b"SCHEMA",
                    b"TRIGGER",
                    b"SEQUENCE",
                ]
                .iter()
                .any(|word| self.word(object, word))
                {
                    return Err(unsupported(object));
                }
                return Err(syntax(object));
            }
        };
        self.peek()?;
        self.complete_ddl_node(builder, root, first.span, last)?;
        Ok(())
    }

    pub(super) fn drop_statement(
        &mut self,
        builder: &mut AstBuilder<'_>,
        first: Token,
    ) -> Result<(), Diagnostic> {
        let root = builder.add(NodeKind::DropTable, first.span, None, self.control)?;
        let object = self.take()?;
        match object.kind {
            TokenKind::Keyword(Keyword::Table) => {}
            TokenKind::Keyword(Keyword::Index) => {
                builder.set_kind(root, NodeKind::DropIndex);
            }
            _ => return Err(syntax(object)),
        }
        let last = self.ddl_target(builder, root, ConditionalTarget::Drop)?;
        let next = self.peek()?;
        if next.kind == TokenKind::Comma
            || self.word(next, b"CASCADE")
            || self.word(next, b"RESTRICT")
        {
            return Err(unsupported(next));
        }
        self.complete_ddl_node(builder, root, first.span, last)?;
        Ok(())
    }

    fn ddl_target(
        &mut self,
        builder: &mut AstBuilder<'_>,
        parent: usize,
        conditional: ConditionalTarget,
    ) -> Result<Span, Diagnostic> {
        let first = self.peek()?;
        check(self.control, first.span.start)?;
        let span = self.target(builder, parent)?;
        let next = self.peek()?;
        if next.kind == TokenKind::Dot {
            return Err(unsupported(next));
        }
        // The common target helper has already retained IF as the actual name.
        // PUBLIC.IF and quoted IF do not introduce conditional DDL syntax.
        if span == first.span && self.word(first, b"IF") {
            match conditional {
                ConditionalTarget::Create if next.kind == TokenKind::Keyword(Keyword::Not) => {
                    let not = self.take()?;
                    let exists = self.peek()?;
                    return Err(if self.word(exists, b"EXISTS") {
                        unsupported(first)
                    } else {
                        syntax(not)
                    });
                }
                ConditionalTarget::Drop if self.word(next, b"EXISTS") => {
                    return Err(unsupported(first));
                }
                _ => {}
            }
        }
        check(self.control, span.end)?;
        Ok(span)
    }

    fn ddl_table_definition(
        &mut self,
        builder: &mut AstBuilder<'_>,
        root: usize,
    ) -> Result<Span, Diagnostic> {
        let open = self.expect(TokenKind::LeftParen)?;
        let definition = builder.add(
            NodeKind::TableDefinition,
            open.span,
            Some(root),
            self.control,
        )?;
        let mut has_column = false;
        let mut phase = DefinitionPhase::Columns;
        loop {
            let first = self.peek()?;
            check(self.control, first.span.start)?;
            match first.kind {
                TokenKind::Identifier | TokenKind::QuotedIdentifier => {
                    // A contextual first token is consumed exactly once, and is
                    // still the Name token if the following token is a type.
                    self.take()?;
                    self.ddl_column_feature(first)?;
                    if phase != DefinitionPhase::Columns {
                        return Err(syntax(first));
                    }
                    self.ddl_column(builder, definition, first)?;
                    has_column = true;
                }
                TokenKind::Keyword(Keyword::Primary) => {
                    if !has_column || phase != DefinitionPhase::Columns {
                        return Err(syntax(first));
                    }
                    self.take()?;
                    self.ddl_constraint(builder, definition, first, true)?;
                    phase = DefinitionPhase::PrimaryKey;
                }
                TokenKind::Keyword(Keyword::Unique) => {
                    if !has_column {
                        return Err(syntax(first));
                    }
                    self.take()?;
                    self.ddl_constraint(builder, definition, first, false)?;
                    phase = DefinitionPhase::Unique;
                }
                _ => return Err(syntax(first)),
            }
            if self.peek()?.kind != TokenKind::Comma {
                break;
            }
            let comma = self.take()?;
            check(self.control, comma.span.end)?;
        }
        let close = self.expect(TokenKind::RightParen)?;
        self.complete_ddl_node(builder, definition, open.span, close.span)
    }

    fn ddl_column_feature(&mut self, first: Token) -> Result<(), Diagnostic> {
        if self.word(first, b"CHECK") {
            if self.peek()?.kind == TokenKind::LeftParen {
                return Err(unsupported(first));
            }
        } else if self.word(first, b"FOREIGN") {
            if self.peek()?.kind == TokenKind::Keyword(Keyword::Key) {
                return Err(unsupported(first));
            }
        } else if self.word(first, b"CONSTRAINT")
            && matches!(
                self.peek()?.kind,
                TokenKind::Identifier | TokenKind::QuotedIdentifier
            )
        {
            return Err(unsupported(first));
        }
        Ok(())
    }

    fn ddl_column(
        &mut self,
        builder: &mut AstBuilder<'_>,
        definition: usize,
        first: Token,
    ) -> Result<(), Diagnostic> {
        let column = builder.add(
            NodeKind::ColumnDefinition {
                nullability: NullabilitySyntax::Implicit,
            },
            first.span,
            Some(definition),
            self.control,
        )?;
        let name = normalize_name(self.source, first, self.control)?;
        builder.add(NodeKind::Name(name), first.span, Some(column), self.control)?;
        let mut last = self.ddl_data_type(builder, column)?;
        let nullability = match self.peek()?.kind {
            TokenKind::Keyword(Keyword::Null) => {
                last = self.take()?.span;
                NullabilitySyntax::Null
            }
            TokenKind::Keyword(Keyword::Not) => {
                self.take()?;
                last = self.expect(TokenKind::Keyword(Keyword::Null))?.span;
                NullabilitySyntax::NotNull
            }
            _ => NullabilitySyntax::Implicit,
        };
        let next = self.peek()?;
        if matches!(
            next.kind,
            TokenKind::Keyword(Keyword::Primary | Keyword::Unique)
        ) || [
            b"DEFAULT".as_slice(),
            b"CHECK",
            b"REFERENCES",
            b"CONSTRAINT",
            b"COLLATE",
            b"GENERATED",
        ]
        .iter()
        .any(|word| self.word(next, word))
        {
            return Err(unsupported(next));
        }
        self.complete_ddl_node(builder, column, first.span, last)?;
        builder.set_kind(column, NodeKind::ColumnDefinition { nullability });
        Ok(())
    }

    fn ddl_data_type(
        &mut self,
        builder: &mut AstBuilder<'_>,
        column: usize,
    ) -> Result<Span, Diagnostic> {
        let first = self.take()?;
        let (kind, last) = match first.kind {
            TokenKind::Keyword(Keyword::Bigint) => (TypeSyntax::Int64, first.span),
            TokenKind::Keyword(Keyword::Boolean) => (TypeSyntax::Bool, first.span),
            TokenKind::Keyword(Keyword::Varchar | Keyword::Varbinary) => {
                self.expect(TokenKind::LeftParen)?;
                let length = self.ddl_type_length()?;
                let close = self.expect(TokenKind::RightParen)?;
                let kind = if first.kind == TokenKind::Keyword(Keyword::Varchar) {
                    TypeSyntax::Utf8(length)
                } else {
                    TypeSyntax::Binary(length)
                };
                (kind, close.span)
            }
            TokenKind::Identifier | TokenKind::QuotedIdentifier => {
                return Err(unsupported(first));
            }
            _ => return Err(syntax(first)),
        };
        // No placeholder for a variable type: malformed length syntax wins over
        // its admission. A fixed type is complete before any suffix/nullability peek.
        check(self.control, last.end)?;
        let span = joined(first.span, last);
        builder.add(NodeKind::DataType(kind), span, Some(column), self.control)?;
        Ok(span)
    }

    fn ddl_type_length(&mut self) -> Result<TypeLengthSyntax, Diagnostic> {
        let first = self.take()?;
        let (sign, digits) = match first.kind {
            TokenKind::Digits => (Sign::None, first),
            TokenKind::Plus | TokenKind::Minus => {
                let digits = self.take()?;
                if digits.kind != TokenKind::Digits {
                    return Err(self.expected_scalar(digits));
                }
                (
                    if first.kind == TokenKind::Plus {
                        Sign::Plus
                    } else {
                        Sign::Minus
                    },
                    digits,
                )
            }
            _ => return Err(self.expected_scalar(first)),
        };
        let next = self.peek()?;
        if digits.span.end == next.span.start
            && (next.kind == TokenKind::Dot || self.exponent(next)?)
        {
            return Err(unsupported(next));
        }
        // Keep raw offsets; neither integer conversion nor ScalarReader belongs
        // before DDL precommit and its semantic validation owner.
        Ok(TypeLengthSyntax {
            sign,
            digits: digits.span,
            span: joined(first.span, digits.span),
        })
    }

    fn ddl_constraint(
        &mut self,
        builder: &mut AstBuilder<'_>,
        definition: usize,
        first: Token,
        primary: bool,
    ) -> Result<(), Diagnostic> {
        let constraint = builder.add(
            if primary {
                NodeKind::PrimaryKey
            } else {
                NodeKind::UniqueConstraint
            },
            first.span,
            Some(definition),
            self.control,
        )?;
        if primary {
            self.expect(TokenKind::Keyword(Keyword::Key))?;
        }
        let last = self.ddl_key_columns(builder, constraint, false)?;
        self.complete_ddl_node(builder, constraint, first.span, last)?;
        Ok(())
    }

    fn ddl_key_columns(
        &mut self,
        builder: &mut AstBuilder<'_>,
        parent: usize,
        index: bool,
    ) -> Result<Span, Diagnostic> {
        let open = self.expect(TokenKind::LeftParen)?;
        let columns = builder.add(NodeKind::Columns, open.span, Some(parent), self.control)?;
        loop {
            let first = self.peek()?;
            check(self.control, first.span.start)?;
            if index && index_expression_start(first.kind) {
                return Err(unsupported(first));
            }
            let name = self.name(builder, columns)?;
            let next = self.peek()?;
            if index && index_key_suffix(next.kind) {
                return Err(unsupported(next));
            }
            check(self.control, name.end)?;
            if next.kind != TokenKind::Comma {
                break;
            }
            let comma = self.take()?;
            check(self.control, comma.span.end)?;
        }
        let close = self.expect(TokenKind::RightParen)?;
        self.complete_ddl_node(builder, columns, open.span, close.span)
    }

    fn ddl_storage(
        &mut self,
        builder: &mut AstBuilder<'_>,
        root: usize,
    ) -> Result<Span, Diagnostic> {
        let first = self.expect(TokenKind::Keyword(Keyword::Storage))?;
        let storage = builder.add(NodeKind::StorageDisk, first.span, Some(root), self.control)?;
        let disk = self.take()?;
        match disk.kind {
            TokenKind::Keyword(Keyword::Disk) => {}
            TokenKind::Identifier | TokenKind::QuotedIdentifier => {
                return Err(unsupported(disk));
            }
            _ => return Err(syntax(disk)),
        }
        self.peek()?;
        self.complete_ddl_node(builder, storage, first.span, disk.span)
    }

    fn complete_ddl_node(
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

fn index_expression_start(kind: TokenKind) -> bool {
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

fn index_key_suffix(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Dot
            | TokenKind::LeftParen
            | TokenKind::Plus
            | TokenKind::Minus
            | TokenKind::Star
            | TokenKind::Keyword(Keyword::Asc | Keyword::Desc | Keyword::Nulls)
    )
}
