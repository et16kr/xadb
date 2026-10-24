//! Bounded lexical input and names. See D4-FE-LEX-001 in
//! `devdoc/contracts/dev4-independent-frontend.md` for limits and diagnostics.

mod control;
mod diagnostic;
mod lexer;
mod name;
mod source;
mod splitter;

pub use control::CallControl;
pub use diagnostic::{Code, Diagnostic, Module, Span};
pub use lexer::{Keyword, Lexer, Token, TokenKind};
pub use name::{Name, normalize_name};
pub use source::StatementSource;
pub use splitter::{SplitOutcome, SplitStep, Splitter};
