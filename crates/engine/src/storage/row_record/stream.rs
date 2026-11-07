//! Canonical row bytes without the whole row in memory (dev3-row-overflow.md
//! #plan, #forward-check): a digest of the canonical bytes, one window of
//! them, and a streaming check that judges a chunked row exactly as
//! `decode_canonical_row_with_schema` judges the joined bytes. None of these
//! keeps a row-sized buffer or acquires storage authority.

use super::{
    MAX_ROW_BYTES, MAX_ROW_COLUMNS, RowError, RowSchemaInput, RowValuesInput, ValueRef,
    canonical_row_input_len, matches,
};
use foundation::{codec::ContentHasher, value::MAX_VALUE_BYTES};

/// Emit the canonical bytes of already validated values in order, as the
/// pieces `encode_canonical_row_input` writes: the column count, then each
/// value's tag, length and payload.
fn emit(values: RowValuesInput<'_>, sink: &mut impl FnMut(&[u8])) {
    sink(&(values.len() as u32).to_le_bytes());
    for index in 0..values.len() {
        match values.value(index) {
            ValueRef::Null => sink(&[0]),
            ValueRef::Bool(value) => sink(&[1, u8::from(value)]),
            ValueRef::Int64(value) => {
                sink(&[2]);
                sink(&value.to_le_bytes());
            }
            ValueRef::Utf8(value) => {
                sink(&[3]);
                sink(&(value.len() as u32).to_le_bytes());
                sink(value.as_bytes());
            }
            ValueRef::Binary(value) => {
                sink(&[4]);
                sink(&(value.len() as u32).to_le_bytes());
                sink(value);
            }
        }
    }
}

/// The canonical length and SHA-256 of `values`, with the same checks and
/// error order as `encode_canonical_row_borrowed` and no output buffer.
pub(crate) fn canonical_row_digest_borrowed(
    schema: RowSchemaInput<'_>,
    values: &[ValueRef<'_>],
) -> Result<(usize, [u8; 32]), RowError> {
    let values = RowValuesInput::Borrowed(values);
    let total = canonical_row_input_len(schema, values)?;
    let mut hasher = ContentHasher::new();
    emit(values, &mut |piece| hasher.update(piece));
    Ok((total, hasher.digest()))
}

/// Write the canonical bytes `[start, start + n)` of `values` into
/// `output[..n]`, where `n = min(output.len(), total − start)`, and return n.
/// The checks and their order are those of `encode_canonical_row_borrowed`,
/// then `start < total`; every error leaves `output` unchanged and success
/// leaves its tail unchanged.
pub(crate) fn encode_canonical_window_borrowed(
    schema: RowSchemaInput<'_>,
    values: &[ValueRef<'_>],
    start: usize,
    output: &mut [u8],
) -> Result<usize, RowError> {
    let values = RowValuesInput::Borrowed(values);
    let total = canonical_row_input_len(schema, values)?;
    if start >= total || output.is_empty() {
        return Err(RowError::InvalidInput);
    }
    let end = total.min(start + output.len());
    let mut at = 0_usize;
    emit(values, &mut |piece| {
        let (from, to) = (at, at + piece.len());
        at = to;
        let (lo, hi) = (from.max(start), to.min(end));
        if lo < hi {
            output[lo - start..hi - start].copy_from_slice(&piece[lo - from..hi - from]);
        }
    });
    debug_assert_eq!(at, total);
    Ok(end - start)
}

/// A UTF-8 sequence cut by a chunk boundary: at most three leading bytes.
#[derive(Clone, Copy, Default)]
struct Utf8Carry {
    bytes: [u8; 4],
    len: usize,
}

fn utf8_width(lead: u8) -> usize {
    match lead {
        0x00..=0x7f => 1,
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => 0,
    }
}

impl Utf8Carry {
    /// Check the next bytes of one UTF-8 value. A sequence cut at the end of
    /// `piece` is kept only while its prefix is valid; `std::str::from_utf8`
    /// judges every complete sequence, as the joined decoder does.
    fn feed(&mut self, mut piece: &[u8]) -> Result<(), RowError> {
        if self.len > 0 {
            let width = utf8_width(self.bytes[0]);
            let take = (width - self.len).min(piece.len());
            self.bytes[self.len..self.len + take].copy_from_slice(&piece[..take]);
            self.len += take;
            piece = &piece[take..];
            match std::str::from_utf8(&self.bytes[..self.len]) {
                Ok(_) => self.len = 0,
                Err(error) if error.error_len().is_none() && self.len < width => return Ok(()),
                Err(_) => return Err(RowError::Corrupt),
            }
        }
        match std::str::from_utf8(piece) {
            Ok(_) => Ok(()),
            Err(error) if error.error_len().is_none() => {
                let tail = &piece[error.valid_up_to()..];
                self.bytes[..tail.len()].copy_from_slice(tail);
                self.len = tail.len();
                Ok(())
            }
            Err(_) => Err(RowError::Corrupt),
        }
    }
}

#[derive(Clone, Copy)]
enum Field {
    /// The 4-byte column count.
    Count,
    /// A value's tag byte.
    Tag,
    /// The fixed bytes after a tag: Bool 1, Int64 8, Utf8/Binary length 4.
    Fixed { tag: u8, need: usize },
    /// A Utf8/Binary payload with `left` bytes still to come.
    Payload { tag: u8, left: usize },
    /// Every column was read; no byte may follow.
    End,
}

/// Judges a canonical row given in pieces of any size, with the result and
/// error that `decode_canonical_row_with_schema(joined, schema)` gives for
/// the joined bytes of known length `total`. A failed push leaves the stream
/// failed; it keeps at most 8 bytes of a cut field and 3 of a cut character.
pub(crate) struct CanonicalRowStream<'a> {
    schema: RowSchemaInput<'a>,
    total: usize,
    seen: usize,
    columns: usize,
    column: usize,
    field: Field,
    fixed: [u8; 8],
    filled: usize,
    utf8: Utf8Carry,
    failed: Option<RowError>,
}

impl<'a> CanonicalRowStream<'a> {
    /// The checks the joined decoder makes before it reads a byte.
    pub(crate) fn new(schema: RowSchemaInput<'a>, total: usize) -> Result<Self, RowError> {
        schema.validate()?;
        if total > MAX_ROW_BYTES {
            return Err(RowError::TooLarge);
        }
        if total < 4 {
            return Err(RowError::Corrupt);
        }
        Ok(Self {
            schema,
            total,
            seen: 0,
            columns: 0,
            column: 0,
            field: Field::Count,
            fixed: [0; 8],
            filled: 0,
            utf8: Utf8Carry::default(),
            failed: None,
        })
    }

    pub(crate) fn push(&mut self, piece: &[u8]) -> Result<(), RowError> {
        if let Some(error) = self.failed {
            return Err(error);
        }
        if piece.len() > self.total - self.seen {
            return Err(RowError::InvalidInput);
        }
        let result = self.consume(piece);
        if let Err(error) = result {
            self.failed = Some(error);
        }
        result
    }

    /// Every byte arrived and the last column ended exactly at `total`.
    pub(crate) fn finish(&self) -> Result<(), RowError> {
        if let Some(error) = self.failed {
            return Err(error);
        }
        if self.seen != self.total {
            return Err(RowError::InvalidInput);
        }
        match self.field {
            Field::End => Ok(()),
            _ => Err(RowError::Corrupt),
        }
    }

    fn remaining(&self) -> usize {
        self.total - self.seen
    }

    fn consume(&mut self, mut piece: &[u8]) -> Result<(), RowError> {
        while !piece.is_empty() {
            let used = match self.field {
                Field::Count | Field::Fixed { .. } => self.fill(piece)?,
                Field::Tag => self.tag(piece[0])?,
                Field::Payload { tag, left } => self.payload(tag, left, piece)?,
                // The joined decoder: offset != input.len().
                Field::End => return Err(RowError::Corrupt),
            };
            self.seen += used;
            piece = &piece[used..];
        }
        Ok(())
    }

    /// Bytes of the count or of a value's fixed part.
    fn fill(&mut self, piece: &[u8]) -> Result<usize, RowError> {
        let need = match self.field {
            Field::Count => 4,
            Field::Fixed { need, .. } => need,
            _ => unreachable!("fill is only called for fixed fields"),
        };
        let take = (need - self.filled).min(piece.len());
        self.fixed[self.filled..self.filled + take].copy_from_slice(&piece[..take]);
        self.filled += take;
        // `tag` and `new` already checked that the whole field is present.
        if self.filled < need {
            return Ok(take);
        }
        self.filled = 0;
        match self.field {
            Field::Count => {
                let count = u32::from_le_bytes(self.fixed[..4].try_into().expect("4 bytes"));
                let count = count as usize;
                if !(1..=MAX_ROW_COLUMNS).contains(&count) {
                    return Err(RowError::Corrupt);
                }
                if count != self.schema.len() {
                    return Err(RowError::SchemaMismatch);
                }
                self.columns = count;
                self.field = Field::Tag;
            }
            Field::Fixed { tag: 1, .. } => {
                let value = match self.fixed[0] {
                    0 => ValueRef::Bool(false),
                    1 => ValueRef::Bool(true),
                    _ => return Err(RowError::Corrupt),
                };
                self.end_value(value)?;
            }
            Field::Fixed { tag: 2, .. } => self.end_value(ValueRef::Int64(0))?,
            Field::Fixed { tag, .. } => {
                let length = u32::from_le_bytes(self.fixed[..4].try_into().expect("4 bytes"));
                let length = length as usize;
                if length > MAX_VALUE_BYTES {
                    return Err(RowError::TooLarge);
                }
                if length > self.remaining() - take {
                    return Err(RowError::Corrupt);
                }
                if length == 0 {
                    self.end_value(empty(tag))?;
                } else {
                    self.field = Field::Payload { tag, left: length };
                }
            }
            _ => unreachable!("fill is only called for fixed fields"),
        }
        Ok(take)
    }

    fn tag(&mut self, tag: u8) -> Result<usize, RowError> {
        let need = match tag {
            0 => {
                self.end_value(ValueRef::Null)?;
                return Ok(1);
            }
            1 => 1,
            2 => 8,
            3 | 4 => 4,
            _ => return Err(RowError::Corrupt),
        };
        // The joined decoder needs the whole fixed part present.
        if need > self.remaining() - 1 {
            return Err(RowError::Corrupt);
        }
        self.field = Field::Fixed { tag, need };
        Ok(1)
    }

    fn payload(&mut self, tag: u8, left: usize, piece: &[u8]) -> Result<usize, RowError> {
        let take = left.min(piece.len());
        if tag == 3 {
            self.utf8.feed(&piece[..take])?;
        }
        if take < left {
            self.field = Field::Payload {
                tag,
                left: left - take,
            };
            return Ok(take);
        }
        if self.utf8.len != 0 {
            return Err(RowError::Corrupt);
        }
        self.end_value(empty(tag))?;
        Ok(take)
    }

    /// The schema judgement the joined decoder makes after parsing a value.
    /// Only the value's kind matters, so Utf8/Binary use an empty stand-in.
    fn end_value(&mut self, value: ValueRef<'_>) -> Result<(), RowError> {
        if !matches(self.schema.column(self.column)?, value) {
            return Err(RowError::SchemaMismatch);
        }
        self.column += 1;
        self.field = if self.column == self.columns {
            Field::End
        } else {
            Field::Tag
        };
        Ok(())
    }
}

fn empty(tag: u8) -> ValueRef<'static> {
    if tag == 3 {
        ValueRef::Utf8("")
    } else {
        ValueRef::Binary(&[])
    }
}

/// Source models of the A helpers and the stream (dev3-row-overflow.md
/// #o1-record): explicit sums of each frame's typed carriers, not ABI stack
/// samples. `sha` is the caller census's SHA closure.
pub(crate) mod source {
    use super::super::source::{input_len, returned, schema_lookup};
    use super::*;
    use std::mem::size_of;

    /// `emit`: arguments, the index, one value and its fixed bytes, and the
    /// piece handed to the sink.
    fn emit() -> usize {
        size_of::<(RowValuesInput<'_>, &mut ())>()
            + size_of::<usize>()
            + size_of::<ValueRef<'_>>()
            + size_of::<([u8; 8], [u8; 4])>()
            + size_of::<&[u8]>()
    }

    /// `canonical_row_digest_borrowed`: arguments, the values, total, hasher
    /// and its closure, digest and Result, then the length check or `emit`
    /// with the hasher update.
    pub(crate) fn digest(sha: usize) -> usize {
        size_of::<(RowSchemaInput<'_>, &[ValueRef<'_>])>()
            + size_of::<RowValuesInput<'_>>()
            + returned::<usize, RowError>()
            + size_of::<ContentHasher>()
            + size_of::<&mut ContentHasher>()
            + size_of::<[u8; 32]>()
            + size_of::<Result<(usize, [u8; 32]), RowError>>()
            + input_len().max(emit() + sha)
    }

    /// `encode_canonical_window_borrowed`: arguments, the values, total,
    /// end, the closure's captures and its seven offsets, the Result, then
    /// the length check or `emit` with the copy.
    pub(crate) fn window() -> usize {
        size_of::<(RowSchemaInput<'_>, &[ValueRef<'_>], usize, &mut [u8])>()
            + size_of::<RowValuesInput<'_>>()
            + returned::<usize, RowError>()
            + size_of::<(&mut usize, &usize, &usize, &mut [u8])>()
            + size_of::<[usize; 7]>()
            + size_of::<Result<usize, RowError>>()
            + input_len().max(emit())
    }

    /// The stream a caller holds across its chunk loads.
    pub(crate) fn stream() -> usize {
        size_of::<CanonicalRowStream<'static>>()
    }

    /// `CanonicalRowStream::push` → `consume` → `fill`/`tag`/`payload` →
    /// `Utf8Carry::feed` or `end_value` with the schema lookup.
    pub(crate) fn push() -> usize {
        let push = size_of::<(&mut CanonicalRowStream<'_>, &[u8])>()
            + 2 * size_of::<Result<(), RowError>>();
        let consume = size_of::<&[u8]>() + returned::<usize, RowError>();
        let fill = size_of::<(&mut CanonicalRowStream<'_>, &[u8])>()
            + size_of::<[usize; 3]>()
            + size_of::<ValueRef<'_>>()
            + size_of::<Result<usize, RowError>>();
        let tag = size_of::<(&mut CanonicalRowStream<'_>, u8)>()
            + size_of::<usize>()
            + size_of::<Result<usize, RowError>>();
        let payload = size_of::<(&mut CanonicalRowStream<'_>, u8, usize, &[u8])>()
            + size_of::<usize>()
            + size_of::<Result<usize, RowError>>();
        let feed = size_of::<(&mut Utf8Carry, &[u8])>()
            + size_of::<[usize; 3]>()
            + 2 * size_of::<Result<&str, std::str::Utf8Error>>()
            + size_of::<&[u8]>()
            + size_of::<Result<(), RowError>>();
        let end_value = size_of::<(&mut CanonicalRowStream<'_>, ValueRef<'_>)>()
            + size_of::<Result<(), RowError>>()
            + schema_lookup();
        push + consume + fill.max(tag).max(payload) + feed.max(end_value)
    }
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod tests;
