//! Allocation-free canonical state and raw-input codecs for the frozen lifecycle schema.
use super::input_schema::ResolvedInput;
use super::schema::{State, WireIdentity};
use super::workspace::{MAX_INPUT_BYTES, MAX_STATE_BYTES};
use sha2::{Digest, Sha256};

#[path = "codec_records.rs"]
mod records;
#[cfg(test)]
#[path = "codec_tests.rs"]
mod tests;
#[path = "codec_validate.rs"]
mod validate;

/// Structural failure, distinct from an API rejection or an environment exclusion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodecError {
    /// Byte offset within the currently checked bounded slice; state relation errors use zero.
    pub offset: usize,
    /// Stable, allocation-free description of the failed structural requirement.
    pub reason: &'static str,
}
impl CodecError {
    fn new(offset: usize, reason: &'static str) -> Self {
        Self { offset, reason }
    }
}
fn ensure(valid: bool, offset: usize, reason: &'static str) -> Result<(), CodecError> {
    if valid {
        Ok(())
    } else {
        Err(CodecError::new(offset, reason))
    }
}
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}
impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }
    fn take(&mut self, len: usize) -> Result<&'a [u8], CodecError> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or(CodecError::new(self.pos, "offset overflow"))?;
        let bytes = self
            .bytes
            .get(self.pos..end)
            .ok_or(CodecError::new(self.pos, "truncated bytes"))?;
        self.pos = end;
        Ok(bytes)
    }
    fn byte(&mut self) -> Result<u8, CodecError> {
        Ok(self.take(1)?[0])
    }
    fn word(&mut self) -> Result<u16, CodecError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn dword(&mut self) -> Result<u32, CodecError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn zero(&mut self, len: usize) -> Result<(), CodecError> {
        let at = self.pos;
        ensure(
            self.take(len)?.iter().all(|b| *b == 0),
            at,
            "nonzero reserved bytes",
        )
    }
    fn end(&self) -> Result<(), CodecError> {
        ensure(self.pos == self.bytes.len(), self.pos, "trailing bytes")
    }
}
struct Writer<'a> {
    bytes: &'a mut [u8],
    pos: usize,
}
impl<'a> Writer<'a> {
    fn new(bytes: &'a mut [u8]) -> Self {
        Self { bytes, pos: 0 }
    }
    fn put(&mut self, value: &[u8]) -> Result<(), CodecError> {
        let end = self
            .pos
            .checked_add(value.len())
            .ok_or(CodecError::new(self.pos, "offset overflow"))?;
        let dst = self
            .bytes
            .get_mut(self.pos..end)
            .ok_or(CodecError::new(self.pos, "output buffer too small"))?;
        dst.copy_from_slice(value);
        self.pos = end;
        Ok(())
    }
    fn byte(&mut self, value: u8) -> Result<(), CodecError> {
        self.put(&[value])
    }
    fn word(&mut self, value: u16) -> Result<(), CodecError> {
        self.put(&value.to_le_bytes())
    }
    fn dword(&mut self, value: u32) -> Result<(), CodecError> {
        self.put(&value.to_le_bytes())
    }
    fn zero(&mut self, len: usize) -> Result<(), CodecError> {
        for _ in 0..len {
            self.byte(0)?;
        }
        Ok(())
    }
    fn patch_word(&mut self, at: usize, value: u16) -> Result<(), CodecError> {
        let end = at
            .checked_add(2)
            .ok_or(CodecError::new(at, "offset overflow"))?;
        let dst = self
            .bytes
            .get_mut(at..end)
            .ok_or(CodecError::new(at, "patch beyond output"))?;
        dst.copy_from_slice(&value.to_le_bytes());
        Ok(())
    }
}
trait Field: Sized {
    fn read(r: &mut Reader<'_>) -> Result<Self, CodecError>;
    fn write(&self, w: &mut Writer<'_>) -> Result<(), CodecError>;
}
impl Field for u8 {
    fn read(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        r.byte()
    }
    fn write(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        w.byte(*self)
    }
}
impl Field for u16 {
    fn read(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        r.word()
    }
    fn write(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        w.word(*self)
    }
}
impl Field for bool {
    fn read(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        match r.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(CodecError::new(r.pos - 1, "invalid boolean")),
        }
    }
    fn write(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        w.byte(u8::from(*self))
    }
}
impl<T: Field> Field for Option<T> {
    fn read(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        match r.byte()? {
            0 => Ok(None),
            1 => Ok(Some(T::read(r)?)),
            _ => Err(CodecError::new(r.pos - 1, "invalid presence tag")),
        }
    }
    fn write(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        match self {
            None => w.byte(0),
            Some(v) => {
                w.byte(1)?;
                v.write(w)
            }
        }
    }
}

/// Encode a validated canonical state without allocation.
/// On failure the output slice is uncommitted scratch and must not be published.
pub fn encode_state(
    state: &State,
    identity: &WireIdentity,
    out: &mut [u8],
) -> Result<usize, CodecError> {
    validate::state(state)?;
    let cap = out.len().min(MAX_STATE_BYTES);
    let mut w = Writer::new(&mut out[..cap]);
    w.put(b"XADBLM02")?;
    w.word(2)?;
    w.word(256)?;
    w.dword(0)?;
    for digest in [
        &identity.layout,
        &identity.profile,
        &identity.action,
        &identity.exclusion,
    ] {
        w.put(digest)?;
    }
    state.current_engine.write(&mut w)?;
    for n in state.next_nonce {
        w.word(n)?;
    }
    for n in records::counts(state) {
        w.word(n as u16)?;
    }
    w.zero(54)?;
    records::write_sections(state, &mut w)?;
    let end = w
        .pos
        .checked_add(32)
        .ok_or(CodecError::new(w.pos, "state size overflow"))?;
    ensure(end <= cap, w.pos, "output buffer too small")?;
    w.bytes[12..16].copy_from_slice(&(end as u32).to_le_bytes());
    let checksum = Sha256::digest(&w.bytes[..w.pos]);
    w.put(&checksum)?;
    Ok(w.pos)
}

/// Decode into reusable caller storage without allocating or normalizing input.
/// A failed decode leaves `out` unvalidated; callers must not publish or use it as a state.
pub fn decode_state(
    bytes: &[u8],
    identity: &WireIdentity,
    out: &mut State,
) -> Result<(), CodecError> {
    ensure(
        (288..=MAX_STATE_BYTES).contains(&bytes.len()),
        0,
        "state byte bound",
    )?;
    let body_len = bytes.len() - 32;
    let actual = Sha256::digest(&bytes[..body_len]);
    ensure(actual[..] == bytes[body_len..], body_len, "state checksum")?;
    let mut r = Reader::new(&bytes[..body_len]);
    ensure(r.take(8)? == b"XADBLM02", 0, "state magic")?;
    ensure(r.word()? == 2, 8, "state version")?;
    ensure(r.word()? == 256, 10, "state header size")?;
    ensure(r.dword()? as usize == bytes.len(), 12, "state total size")?;
    for expected in [
        &identity.layout,
        &identity.profile,
        &identity.action,
        &identity.exclusion,
    ] {
        let at = r.pos;
        ensure(r.take(32)? == expected, at, "wire identity mismatch")?;
    }
    out.current_engine = Field::read(&mut r)?;
    for n in &mut out.next_nonce {
        *n = r.word()?;
    }
    let mut counts = [0; 16];
    for n in &mut counts {
        *n = usize::from(r.word()?);
    }
    r.zero(54)?;
    records::read_sections(&mut r, out, counts)?;
    r.end()?;
    validate::state(out)
}

/// Encode raw input framing while retaining malformed identities, byte values and member order.
/// Environment admission and API semantics are deliberately outside this function.
pub fn encode_input(input: &ResolvedInput, out: &mut [u8]) -> Result<usize, CodecError> {
    validate::input(input)?;
    let cap = out.len().min(MAX_INPUT_BYTES);
    let mut w = Writer::new(&mut out[..cap]);
    w.word(1)?;
    input.api.write(&mut w)?;
    w.byte(input.actor)?;
    w.byte(input.outcome)?;
    w.byte(input.prepare_slot)?;
    w.byte(input.prepare_shape)?;
    w.byte(input.prepare_n)?;
    w.byte(input.set_autocommit)?;
    input.target.write(&mut w)?;
    input.context.write(&mut w)?;
    input.original_deadline.write(&mut w)?;
    input.cancel.write(&mut w)?;
    input.deadline_reached.write(&mut w)?;
    w.byte(input.bind_values.len() as u8)?;
    for value in input.bind_values.as_slice() {
        value.write(&mut w)?;
    }
    input.environment.write(&mut w)?;
    Ok(w.pos)
}

/// Decode only raw container structure, preserving negative identities, values and member order.
/// On failure `out` is unvalidated scratch and must not be submitted to the model.
pub fn decode_input(bytes: &[u8], out: &mut ResolvedInput) -> Result<(), CodecError> {
    ensure(bytes.len() <= MAX_INPUT_BYTES, 0, "input byte bound")?;
    let mut r = Reader::new(bytes);
    ensure(r.word()? == 1, 0, "input version")?;
    out.api = Field::read(&mut r)?;
    out.actor = r.byte()?;
    out.outcome = r.byte()?;
    out.prepare_slot = r.byte()?;
    out.prepare_shape = r.byte()?;
    out.prepare_n = r.byte()?;
    out.set_autocommit = r.byte()?;
    out.target = Field::read(&mut r)?;
    out.context = Field::read(&mut r)?;
    out.original_deadline = Field::read(&mut r)?;
    out.cancel = Field::read(&mut r)?;
    out.deadline_reached = Field::read(&mut r)?;
    let n = usize::from(r.byte()?);
    ensure(n <= 2, r.pos, "binding count cap")?;
    out.bind_values.clear();
    for _ in 0..n {
        let value = Field::read(&mut r)?;
        out.bind_values
            .try_push(value)
            .map_err(|_| CodecError::new(r.pos, "binding count cap"))?;
    }
    out.environment = Field::read(&mut r)?;
    r.end()?;
    validate::input(out)
}
