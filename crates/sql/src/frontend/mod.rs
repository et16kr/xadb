//! Bounded lexical input and names. See D4-FE-LEX-001 in
//! `devdoc/contracts/dev4-independent-frontend.md` for limits and diagnostics.

mod control;
mod diagnostic;
mod lexer;
mod name;

pub use control::CallControl;
pub use diagnostic::{Code, Diagnostic, Module, Span};
pub use lexer::{Keyword, Lexer, Token, TokenKind};
pub use name::{Name, normalize_name};
