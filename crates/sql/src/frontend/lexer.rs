//! Borrowed, allocation-free SQL token scanning.
//!
//! The scanner validates the complete UTF-8 source before exposing a token, keeps
//! original byte spans, and never performs numeric conversion or catalog access.
//! D4-FE-LEX-001 in `devdoc/contracts/dev4-independent-frontend.md` owns the
//! input, checkpoint, terminal-error, and fixed-space contracts. Statement count,
//! binary payload validation, and unsupported numeric forms belong to later
//! parser/scalar consumers, not this token stream.

use super::{
    control::CallControl,
    diagnostic::{Code, Diagnostic, Module, Span},
};

const MAX_SOURCE_BYTES: usize = 1 << 20;
const MAX_TOKENS: usize = 65_536;
const CHECKPOINT_BYTES: usize = 256;

/// Reserved words of the current SQL grammar, matched using ASCII case folding.
///
/// Contextual names such as PUBLIC and future feature words remain identifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keyword {
    /// CREATE begins a table or index definition.
    Create,
    /// TABLE identifies a table in CREATE or DROP.
    Table,
    /// PRIMARY introduces a primary-key constraint.
    Primary,
    /// KEY completes PRIMARY KEY.
    Key,
    /// UNIQUE introduces a uniqueness constraint or unique index.
    Unique,
    /// STORAGE introduces the table storage clause.
    Storage,
    /// DISK selects the supported table storage kind.
    Disk,
    /// BIGINT names the signed 64-bit integer SQL type.
    Bigint,
    /// BOOLEAN names the boolean SQL type.
    Boolean,
    /// VARCHAR names the UTF-8 text SQL type.
    Varchar,
    /// VARBINARY names the byte-string SQL type.
    Varbinary,
    /// INDEX identifies an index in CREATE or DROP.
    Index,
    /// ON introduces an indexed table or enables AUTOCOMMIT.
    On,
    /// DROP begins removal of a table or index.
    Drop,
    /// INSERT begins an insertion statement.
    Insert,
    /// INTO introduces the insertion target.
    Into,
    /// VALUES introduces the insertion tuples.
    Values,
    /// UPDATE begins an update statement.
    Update,
    /// SET introduces assignments or the AUTOCOMMIT setting.
    Set,
    /// DELETE begins a deletion statement.
    Delete,
    /// FROM introduces a SELECT or DELETE input table.
    From,
    /// SELECT begins a query statement.
    Select,
    /// AS introduces an explicit output alias.
    As,
    /// WHERE introduces a predicate.
    Where,
    /// ORDER introduces the ORDER BY clause.
    Order,
    /// BY completes ORDER BY.
    By,
    /// ASC selects ascending order.
    Asc,
    /// DESC selects descending order.
    Desc,
    /// NULLS introduces an explicit null placement.
    Nulls,
    /// FIRST places nulls before non-null values.
    First,
    /// LAST places nulls after non-null values.
    Last,
    /// LIMIT introduces the result row limit.
    Limit,
    /// NOT negates a predicate or modifies NULL tests and constraints.
    Not,
    /// AND combines predicates by conjunction.
    And,
    /// OR combines predicates by disjunction.
    Or,
    /// IS introduces an IS NULL or IS NOT NULL test.
    Is,
    /// NULL denotes a null literal or a nullability/test clause.
    Null,
    /// TRUE denotes the true boolean literal.
    True,
    /// FALSE denotes the false boolean literal.
    False,
    /// BEGIN denotes the transaction-begin statement.
    Begin,
    /// COMMIT denotes the transaction-commit statement.
    Commit,
    /// ROLLBACK denotes the transaction-rollback statement.
    Rollback,
    /// AUTOCOMMIT names the SET AUTOCOMMIT setting.
    Autocommit,
    /// OFF disables AUTOCOMMIT.
    Off,
}

/// Lexical classification without decoded names, numbers, or literal payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    /// One of the current grammar's ASCII case-insensitive reserved words.
    Keyword(Keyword),
    /// An unquoted ASCII name, retaining its original case in the source.
    Identifier,
    /// A double-quoted name whose length and decoded contents are not yet checked.
    QuotedIdentifier,
    /// An unsigned decimal digit run, without conversion or range validation.
    Digits,
    /// A single-quoted text literal, without decoding or empty-text normalization.
    Text,
    /// Contiguous X/x and a quoted payload, without hex or length validation.
    Binary,
    /// The `+` punctuation; numeric signs are combined by the parser.
    Plus,
    /// The `-` punctuation outside a line-comment opener.
    Minus,
    /// The `=` comparison punctuation.
    Equal,
    /// The supported `<>` inequality punctuation.
    NotEqual,
    /// The `<` comparison punctuation.
    Less,
    /// The `<=` comparison punctuation.
    LessEqual,
    /// The `>` comparison punctuation.
    Greater,
    /// The `>=` comparison punctuation.
    GreaterEqual,
    /// The `(` opening punctuation.
    LeftParen,
    /// The `)` closing punctuation.
    RightParen,
    /// The `,` list separator.
    Comma,
    /// The `.` punctuation, retaining contextual numeric or namespace meaning.
    Dot,
    /// The `*` punctuation outside a block comment.
    Star,
    /// The `;` statement delimiter outside quotes and comments.
    Semicolon,
    /// A `?` parameter marker whose ordinal is assigned by the parser.
    Marker,
    /// End of source, with an empty span at the source byte length.
    Eof,
}

/// A token's kind and original UTF-8 byte range, including any quote delimiters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    /// Lexical classification without semantic conversion.
    pub kind: TokenKind,
    /// Original half-open byte range; EOF has equal start and end offsets.
    pub span: Span,
}

/// Sequential scanner borrowing its source and the current call's control.
///
/// No heap storage, recursion, or DB effects are used. The caller keeps both
/// inputs alive and supplies nonblocking, allocation-free control dependencies.
/// Once an error is observed, every later request returns that exact diagnostic.
/// Successful EOF can be requested repeatedly; each request still checks control.
pub struct Lexer<'source, 'call> {
    source: &'source [u8],
    control: &'call CallControl<'call>,
    position: usize,
    token_count: usize,
    last_checkpoint: usize,
    terminal_error: Option<Diagnostic>,
}

impl<'source, 'call> Lexer<'source, 'call> {
    /// Validate a borrowed source and begin scanning at byte zero.
    ///
    /// # Errors
    /// Checks cancellation/deadline first, then the 1 MiB source limit, then
    /// complete UTF-8 validity. UTF-8 checking has bounded progress checkpoints;
    /// invalid input reports the first sequence using `Utf8Error` byte semantics.
    /// This does not validate statement grammar or require a nonempty statement.
    pub fn new(
        source: &'source [u8],
        control: &'call CallControl<'call>,
    ) -> Result<Self, Diagnostic> {
        check_at(control, 0)?;
        if source.len() > MAX_SOURCE_BYTES {
            return Err(diagnostic(Code::Limit, MAX_SOURCE_BYTES, source.len()));
        }
        validate_utf8(source, control)?;
        Ok(Self {
            source,
            control,
            position: 0,
            token_count: 0,
            last_checkpoint: 0,
            terminal_error: None,
        })
    }

    /// Return the next nontrivia token, or EOF at `[source.len(), source.len())`.
    ///
    /// # Errors
    /// Returns Syntax for unknown input or unfinished quotes/block comments,
    /// Unsupported for `!=`, Limit before the 65,537th nontrivia token, or the
    /// current cancellation/deadline diagnostic. Any error makes this scanner
    /// terminal and is returned unchanged by subsequent calls. Previously
    /// returned tokens remain observations, not a complete-statement success.
    pub fn next_token(&mut self) -> Result<Token, Diagnostic> {
        if let Some(error) = self.terminal_error {
            return Err(error);
        }
        let result = self.scan_token();
        if let Err(error) = result {
            self.terminal_error = Some(error);
        }
        result
    }

    fn scan_token(&mut self) -> Result<Token, Diagnostic> {
        self.checkpoint()?;
        self.skip_trivia()?;
        self.checkpoint()?;
        let start = self.position;
        let Some(&first) = self.source.get(start) else {
            return self.finish_token(TokenKind::Eof, start);
        };
        if self.token_count == MAX_TOKENS {
            return Err(diagnostic(Code::Limit, start, start));
        }

        let kind = match first {
            b'x' | b'X' if self.source.get(start + 1) == Some(&b'\'') => {
                self.advance()?;
                self.scan_quote(b'\'', start)?;
                TokenKind::Binary
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                while self
                    .source
                    .get(self.position)
                    .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
                {
                    self.advance()?;
                }
                keyword(&self.source[start..self.position])
                    .map_or(TokenKind::Identifier, TokenKind::Keyword)
            }
            b'0'..=b'9' => {
                while self
                    .source
                    .get(self.position)
                    .is_some_and(u8::is_ascii_digit)
                {
                    self.advance()?;
                }
                TokenKind::Digits
            }
            b'\'' => {
                self.scan_quote(b'\'', start)?;
                TokenKind::Text
            }
            b'"' => {
                self.scan_quote(b'"', start)?;
                TokenKind::QuotedIdentifier
            }
            b'!' if self.source.get(start + 1) == Some(&b'=') => {
                return Err(diagnostic(Code::Unsupported, start, start + 2));
            }
            b'<' => {
                self.advance()?;
                match self.source.get(self.position) {
                    Some(b'=') => {
                        self.advance()?;
                        TokenKind::LessEqual
                    }
                    Some(b'>') => {
                        self.advance()?;
                        TokenKind::NotEqual
                    }
                    _ => TokenKind::Less,
                }
            }
            b'>' => {
                self.advance()?;
                if self.source.get(self.position) == Some(&b'=') {
                    self.advance()?;
                    TokenKind::GreaterEqual
                } else {
                    TokenKind::Greater
                }
            }
            _ => {
                let kind = match first {
                    b'+' => TokenKind::Plus,
                    b'-' => TokenKind::Minus,
                    b'=' => TokenKind::Equal,
                    b'(' => TokenKind::LeftParen,
                    b')' => TokenKind::RightParen,
                    b',' => TokenKind::Comma,
                    b'.' => TokenKind::Dot,
                    b'*' => TokenKind::Star,
                    b';' => TokenKind::Semicolon,
                    b'?' => TokenKind::Marker,
                    _ => {
                        // Complete source validity and ASCII delimiters guarantee
                        // this position starts one whole Unicode scalar.
                        let width = if first < 0x80 {
                            1
                        } else if first < 0xE0 {
                            2
                        } else if first < 0xF0 {
                            3
                        } else {
                            4
                        };
                        return Err(diagnostic(Code::Syntax, start, start + width));
                    }
                };
                self.advance()?;
                kind
            }
        };
        self.finish_token(kind, start)
    }

    fn skip_trivia(&mut self) -> Result<(), Diagnostic> {
        loop {
            while self
                .source
                .get(self.position)
                .is_some_and(|byte| matches!(*byte, b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r'))
            {
                self.advance()?;
            }
            let start = self.position;
            if self.source.get(start..start + 2) == Some(b"--") {
                self.advance()?;
                self.advance()?;
                while self
                    .source
                    .get(self.position)
                    .is_some_and(|byte| !matches!(*byte, b'\r' | b'\n'))
                {
                    self.advance()?;
                }
            } else if self.source.get(start..start + 2) == Some(b"/*") {
                self.advance()?;
                self.advance()?;
                loop {
                    if self.source.get(self.position..self.position + 2) == Some(b"*/") {
                        self.advance()?;
                        self.advance()?;
                        break;
                    }
                    if self.position == self.source.len() {
                        return Err(diagnostic(Code::Syntax, start, self.position));
                    }
                    self.advance()?;
                }
            } else {
                return Ok(());
            }
        }
    }

    fn scan_quote(&mut self, delimiter: u8, start: usize) -> Result<(), Diagnostic> {
        self.advance()?;
        while let Some(&byte) = self.source.get(self.position) {
            self.advance()?;
            if byte == delimiter {
                if self.source.get(self.position) == Some(&delimiter) {
                    self.advance()?;
                } else {
                    return Ok(());
                }
            }
        }
        Err(diagnostic(Code::Syntax, start, self.position))
    }

    fn finish_token(&mut self, kind: TokenKind, start: usize) -> Result<Token, Diagnostic> {
        self.checkpoint()?;
        if kind != TokenKind::Eof {
            self.token_count += 1;
        }
        Ok(Token {
            kind,
            span: Span {
                start,
                end: self.position,
            },
        })
    }

    fn advance(&mut self) -> Result<(), Diagnostic> {
        self.position += 1;
        if self.position - self.last_checkpoint >= CHECKPOINT_BYTES {
            self.checkpoint()?;
        }
        Ok(())
    }

    fn checkpoint(&mut self) -> Result<(), Diagnostic> {
        check_at(self.control, self.position)?;
        self.last_checkpoint = self.position;
        Ok(())
    }
}

fn diagnostic(code: Code, start: usize, end: usize) -> Diagnostic {
    Diagnostic {
        code,
        module: Module::Lex,
        span: Span { start, end },
    }
}

fn check_at(control: &CallControl<'_>, position: usize) -> Result<(), Diagnostic> {
    control.check(
        Module::Lex,
        Span {
            start: position,
            end: position,
        },
    )
}

fn validate_utf8(source: &[u8], control: &CallControl<'_>) -> Result<(), Diagnostic> {
    let mut start = 0;
    while start < source.len() {
        let end = (start + CHECKPOINT_BYTES).min(source.len());
        match std::str::from_utf8(&source[start..end]) {
            Ok(_) => start = end,
            Err(error) => {
                let invalid_start = start + error.valid_up_to();
                if let Some(length) = error.error_len() {
                    return Err(diagnostic(
                        Code::Syntax,
                        invalid_start,
                        invalid_start + length,
                    ));
                }
                if end == source.len() {
                    return Err(diagnostic(Code::Syntax, invalid_start, end));
                }
                // Only an incomplete final scalar is carried into the next
                // bounded window. No input copy or unchecked UTF-8 conversion
                // is needed, and a real invalid sequence retains global offsets.
                start = invalid_start;
            }
        }
        check_at(control, start)?;
    }
    Ok(())
}

/// Shared lexical classification used to reject forged unquoted name tokens.
pub(crate) fn keyword(bytes: &[u8]) -> Option<Keyword> {
    let mut uppercase = [0_u8; 10];
    if bytes.len() > uppercase.len() {
        return None;
    }
    for (output, input) in uppercase.iter_mut().zip(bytes) {
        *output = input.to_ascii_uppercase();
    }
    Some(match &uppercase[..bytes.len()] {
        b"CREATE" => Keyword::Create,
        b"TABLE" => Keyword::Table,
        b"PRIMARY" => Keyword::Primary,
        b"KEY" => Keyword::Key,
        b"UNIQUE" => Keyword::Unique,
        b"STORAGE" => Keyword::Storage,
        b"DISK" => Keyword::Disk,
        b"BIGINT" => Keyword::Bigint,
        b"BOOLEAN" => Keyword::Boolean,
        b"VARCHAR" => Keyword::Varchar,
        b"VARBINARY" => Keyword::Varbinary,
        b"INDEX" => Keyword::Index,
        b"ON" => Keyword::On,
        b"DROP" => Keyword::Drop,
        b"INSERT" => Keyword::Insert,
        b"INTO" => Keyword::Into,
        b"VALUES" => Keyword::Values,
        b"UPDATE" => Keyword::Update,
        b"SET" => Keyword::Set,
        b"DELETE" => Keyword::Delete,
        b"FROM" => Keyword::From,
        b"SELECT" => Keyword::Select,
        b"AS" => Keyword::As,
        b"WHERE" => Keyword::Where,
        b"ORDER" => Keyword::Order,
        b"BY" => Keyword::By,
        b"ASC" => Keyword::Asc,
        b"DESC" => Keyword::Desc,
        b"NULLS" => Keyword::Nulls,
        b"FIRST" => Keyword::First,
        b"LAST" => Keyword::Last,
        b"LIMIT" => Keyword::Limit,
        b"NOT" => Keyword::Not,
        b"AND" => Keyword::And,
        b"OR" => Keyword::Or,
        b"IS" => Keyword::Is,
        b"NULL" => Keyword::Null,
        b"TRUE" => Keyword::True,
        b"FALSE" => Keyword::False,
        b"BEGIN" => Keyword::Begin,
        b"COMMIT" => Keyword::Commit,
        b"ROLLBACK" => Keyword::Rollback,
        b"AUTOCOMMIT" => Keyword::Autocommit,
        b"OFF" => Keyword::Off,
        _ => return None,
    })
}
