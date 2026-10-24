//! Fixed-space SQL identifier normalization, without catalog lookup.

use super::{CallControl, Code, Diagnostic, Module, Span, Token, TokenKind, lexer};

const MAX_NAME_BYTES: usize = 128;
const MAX_RAW_QUOTED_BYTES: usize = MAX_NAME_BYTES * 2 + 2;

/// Owned normalized bytes with source metadata, using no heap allocation.
/// Identifier identity is `as_bytes()` equality. The source span and quoted flag
/// are diagnostic metadata and do not distinguish `t`, `T`, and `"T"`.
#[derive(Debug)]
pub struct Name {
    bytes: [u8; MAX_NAME_BYTES],
    len: usize,
    span: Span,
    quoted: bool,
}

impl Name {
    /// The canonical UTF-8 name, without quote delimiters or doubled escapes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }

    /// Original byte range, before decoding or ASCII uppercasing.
    pub fn span(&self) -> Span {
        self.span
    }

    /// Whether the caller supplied SQL quoted-identifier syntax.
    pub fn is_quoted(&self) -> bool {
        self.quoted
    }
}

/// Normalize one lexical identifier into an independent fixed-space value.
/// No catalog, namespace, quota, allocation or persistent effect is involved.
/// The source outside the token is not inspected. Forged token ranges and kinds
/// are validated; callers do not need to prove the token came from this lexer.
///
/// # Errors
/// Returns Name for invalid ranges, syntax, reserved unquoted words, UTF-8,
/// controls, or a decoded length outside 1..=128 bytes. The whole supplied span
/// is clamped to the source for these errors. Cancellation/deadline takes
/// precedence at entry and is checked during validation and decoding.
pub fn normalize_name(
    source: &[u8],
    token: Token,
    control: &CallControl<'_>,
) -> Result<Name, Diagnostic> {
    let start = token.span.start.min(source.len());
    let span = Span {
        start,
        end: token.span.end.min(source.len()).max(start),
    };
    check(control, start)?;
    let invalid = Diagnostic {
        code: Code::Name,
        module: Module::Name,
        span,
    };
    let raw = source
        .get(token.span.start..token.span.end)
        .ok_or(invalid)?;
    let quoted = match token.kind {
        TokenKind::Identifier => false,
        TokenKind::QuotedIdentifier => true,
        _ => return Err(invalid),
    };
    // No valid name can exceed this raw length: each of 128 decoded bytes
    // can require at most two quote bytes, plus the two outer delimiters.
    if raw.is_empty() || raw.len() > MAX_RAW_QUOTED_BYTES {
        return Err(invalid);
    }
    validate_utf8(raw, start, control, invalid)?;
    let mut name = Name {
        bytes: [0; MAX_NAME_BYTES],
        len: 0,
        span,
        quoted,
    };
    if quoted {
        if raw.len() < 2 || raw[0] != b'"' || raw[raw.len() - 1] != b'"' {
            return Err(invalid);
        }
        let mut position = 1;
        let end = raw.len() - 1;
        let mut last_check = position;
        while position < end {
            let width = match raw[position] {
                0..=0x7f => 1,
                0xc2..=0xdf => 2,
                0xe0..=0xef => 3,
                _ => 4,
            };
            let scalar = raw.get(position..position + width).ok_or(invalid)?;
            let ch = std::str::from_utf8(scalar)
                .map_err(|_| invalid)?
                .chars()
                .next()
                .ok_or(invalid)?;
            if ch.is_control() {
                return Err(invalid);
            }
            if ch == '"' {
                if raw.get(position + 1) != Some(&b'"') || position + 1 >= end {
                    return Err(invalid);
                }
                position += 1;
            }
            if name.len + width > MAX_NAME_BYTES {
                return Err(invalid);
            }
            // The original scalar bytes are canonical; there is no Unicode folding.
            name.bytes[name.len..name.len + width]
                .copy_from_slice(&raw[position..position + width]);
            name.len += width;
            position += width;
            if position - last_check >= 256 {
                check(control, start + position)?;
                last_check = position;
            }
        }
    } else {
        if raw.len() > MAX_NAME_BYTES
            || !(raw[0].is_ascii_alphabetic() || raw[0] == b'_')
            || !raw.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_')
            || lexer::keyword(raw).is_some()
        {
            return Err(invalid);
        }
        for (out, byte) in name.bytes.iter_mut().zip(raw) {
            *out = byte.to_ascii_uppercase();
        }
        name.len = raw.len();
    }
    if name.len == 0 {
        return Err(invalid);
    }
    check(control, span.end)?;
    Ok(name)
}

fn check(control: &CallControl<'_>, position: usize) -> Result<(), Diagnostic> {
    control.check(
        Module::Name,
        Span {
            start: position,
            end: position,
        },
    )
}

fn validate_utf8(
    raw: &[u8],
    source_start: usize,
    control: &CallControl<'_>,
    invalid: Diagnostic,
) -> Result<(), Diagnostic> {
    let mut position = 0;
    while position < raw.len() {
        let end = (position + 256).min(raw.len());
        match std::str::from_utf8(&raw[position..end]) {
            Ok(_) => position = end,
            Err(error) if error.error_len().is_none() && end < raw.len() => {
                position += error.valid_up_to();
            }
            Err(_) => return Err(invalid),
        }
        check(control, source_start + position)?;
    }
    Ok(())
}
