//! Bounded SQL input, names, streaming boundaries and owned raw syntax. See
//! D4-FE-LEX-001, D4-FE-SPLIT-001 and D4-FE-PARSE-001 in
//! `devdoc/contracts/dev4-independent-frontend.md` for limits and diagnostics.

mod ast;
mod control;
mod diagnostic;
mod lexer;
mod name;
mod parse_budget;
mod parser;
mod scalar;
mod source;
mod splitter;

pub use ast::{AstNode, NodeKind, ParsedStatement, ScalarSyntax, Sign};
pub use control::CallControl;
pub use diagnostic::{Code, Diagnostic, Module, Span};
pub use lexer::{Keyword, Lexer, Token, TokenKind};
pub use name::{Name, normalize_name};
pub use parser::Parser;
pub use scalar::{
    DeclaredType, DecodedScalar, OwnedScalar, ScalarReader, ScalarRef, ScalarSpec, ScalarType,
};
pub use source::StatementSource;
pub use splitter::{SplitOutcome, SplitStep, Splitter};
