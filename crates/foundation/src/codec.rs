//! Bounded canonical keys and a current-build frame. This is not a WAL record format.
use crate::{
    Error, ErrorCode, Result,
    value::{MAX_VALUE_BYTES, Value},
};
use sha2::{Digest, Sha256};

/// Maximum current-build frame payload in bytes, excluding header and digest.
pub const MAX_FRAME_PAYLOAD: usize = 1_048_576;
/// AXF1 header size, before payload and digest.
pub const FRAME_HEADER_BYTES: usize = 10;
const HEADER: usize = FRAME_HEADER_BYTES;
/// AXF1 envelope overhead, including the SHA-256 digest.
pub const FRAME_OVERHEAD: usize = 42;
const DIGEST: usize = 32;

/// Streaming SHA-256 state for bounded file-prefix verification.
/// This detects content changes; it is not a signature or an authenticity proof.
/// No file effects or heap buffers are owned by this value.
#[derive(Clone, Default)]
pub struct ContentHasher(Sha256);

impl ContentHasher {
    /// Start an empty content digest, independent of AXF1 framing.
    pub fn new() -> Self {
        Self(Sha256::new())
    }

    /// Include the next ordered bytes without retaining the input.
    pub fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    /// Finish a copy of the current prefix digest, retaining the streaming state.
    pub fn digest(&self) -> [u8; 32] {
        self.0.clone().finalize().into()
    }
}

/// SHA-256 of exactly these bytes, without an AXF1 envelope.
pub fn content_digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn invalid() -> Error {
    Error::new(ErrorCode::InvalidFormat, "invalid frame or checksum")
}

fn allocate(size: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size)
        .map_err(|_| Error::new(ErrorCode::QuotaExceeded, "buffer allocation failed"))?;
    Ok(bytes)
}

/// NULL < bool < signed i64 < UTF-8 < binary. No SQL unique-NULL policy is implied.
/// Returns a new canonical byte key; the input remains borrowed and unchanged.
/// See `key_encoding_matches_hand_calculated_vectors` and the explicit order test.
///
/// # Errors
/// Returns [`ErrorCode::LimitExceeded`] for oversized values or
/// [`ErrorCode::QuotaExceeded`] if output reservation fails.
pub fn encode_key(value: &Value) -> Result<Vec<u8>> {
    let size = encoded_key_len(value)?;
    let mut key = allocate(size)?;
    key.resize(size, 0);
    encode_key_into(value, &mut key)?;
    Ok(key)
}

/// Exact scalar key size after escaping, without allocation or external effects.
///
/// # Errors
/// Returns [`ErrorCode::LimitExceeded`] for a value larger than 64 KiB.
pub fn encoded_key_len(value: &Value) -> Result<usize> {
    value.validate()?;
    let bytes = match value {
        Value::Null => return Ok(1),
        Value::Bool(_) => return Ok(2),
        Value::Int64(_) => return Ok(9),
        Value::Utf8(v) => v.as_bytes(),
        Value::Bytes(v) => v.as_slice(),
    };
    // Value validation bounds this addition even on the smallest supported usize.
    Ok(3 + bytes.len() + bytes.iter().filter(|b| **b == 0).count())
}

/// Encode one canonical scalar into an exactly sized caller buffer.
/// No allocation, I/O, or ownership transfer occurs.
///
/// # Errors
/// Oversized values return [`ErrorCode::LimitExceeded`]; wrong output size returns
/// [`ErrorCode::InvalidFormat`]. Every error preserves the entire output.
pub fn encode_key_into(value: &Value, output: &mut [u8]) -> Result<()> {
    if output.len() != encoded_key_len(value)? {
        return Err(Error::new(ErrorCode::InvalidFormat, "key buffer size"));
    }
    let string = match value {
        Value::Null => {
            output[0] = 0;
            None
        }
        Value::Bool(v) => {
            output.copy_from_slice(&[1, u8::from(*v)]);
            None
        }
        Value::Int64(v) => {
            output[0] = 2;
            output[1..].copy_from_slice(&((*v as u64) ^ (1_u64 << 63)).to_be_bytes());
            None
        }
        Value::Utf8(v) => {
            output[0] = 3;
            Some(v.as_bytes())
        }
        Value::Bytes(v) => {
            output[0] = 4;
            Some(v.as_slice())
        }
    };
    if let Some(bytes) = string {
        let mut offset = 1;
        for b in bytes {
            output[offset] = *b;
            offset += 1;
            if *b == 0 {
                output[offset] = 255;
                offset += 1;
            }
        }
        output[offset..].copy_from_slice(&[0, 0]);
    }
    Ok(())
}

/// Metadata about one validated canonical scalar prefix.
/// It is not a decoded value or a key ownership capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyPrefix {
    /// Bytes occupied by the first scalar, excluding any following scalars.
    pub encoded_len: usize,
    /// Whether the first scalar is NULL.
    pub is_null: bool,
}

/// Inspect the first scalar in concatenated canonical keys without allocation.
/// Validates Bool, escaping, UTF-8, and the decoded 64 KiB value limit.
/// Following bytes are left to the caller to validate or interpret.
///
/// # Errors
/// Truncated/noncanonical input returns [`ErrorCode::InvalidFormat`]; decoded
/// variable payloads over 64 KiB return [`ErrorCode::LimitExceeded`].
pub fn inspect_key_prefix(input: &[u8]) -> Result<KeyPrefix> {
    let malformed = || Error::new(ErrorCode::InvalidFormat, "invalid canonical key");
    let tag = *input.first().ok_or_else(malformed)?;
    let length = match tag {
        0 => 1,
        1 if input.get(1).is_some_and(|v| *v <= 1) => 2,
        2 if input.len() >= 9 => 9,
        3 | 4 => {
            let mut offset = 1;
            let mut segment = 1;
            let mut decoded_len = 0;
            loop {
                let byte = *input.get(offset).ok_or_else(malformed)?;
                if byte != 0 {
                    offset += 1;
                    decoded_len += 1;
                } else {
                    // NUL is an ASCII boundary. Each segment between escaped
                    // NULs must itself be valid UTF-8; no scratch copy is needed.
                    if tag == 3 && std::str::from_utf8(&input[segment..offset]).is_err() {
                        return Err(malformed());
                    }
                    match input.get(offset + 1) {
                        Some(0) => break offset + 2,
                        Some(255) => {
                            decoded_len += 1;
                            offset += 2;
                            segment = offset;
                        }
                        _ => return Err(malformed()),
                    }
                }
                if decoded_len > MAX_VALUE_BYTES {
                    return Err(Error::new(
                        ErrorCode::LimitExceeded,
                        "key value exceeds 64 KiB",
                    ));
                }
            }
        }
        _ => return Err(malformed()),
    };
    Ok(KeyPrefix {
        encoded_len: length,
        is_null: tag == 0,
    })
}

/// AXF1 | format=1 (u16 LE) | payload length (u32 LE) | payload | SHA256(all preceding).
/// Returns an owned frame; checksum detects damage but does not authenticate bytes.
/// See the independent golden vector in `tests/contracts.rs`.
///
/// # Errors
/// Returns [`ErrorCode::LimitExceeded`] above [`MAX_FRAME_PAYLOAD`] or
/// [`ErrorCode::QuotaExceeded`] if output reservation fails. No external effect occurs.
pub fn encode_frame(payload: &[u8]) -> Result<Vec<u8>> {
    if payload.len() > MAX_FRAME_PAYLOAD {
        return Err(Error::new(
            ErrorCode::LimitExceeded,
            "frame payload exceeds 1 MiB",
        ));
    }
    let mut frame = allocate(HEADER + payload.len() + DIGEST)?;
    frame.resize(HEADER + payload.len() + DIGEST, 0);
    encode_frame_into(payload, &mut frame)?;
    Ok(frame)
}

/// Borrowed view: length/format/checksum validated before exposing any payload.
/// The returned slice borrows the input; no allocation or external effect occurs.
/// See `frame_matches_independent_golden_and_rejects_every_truncation_and_bit_flip`.
///
/// # Errors
/// Returns [`ErrorCode::InvalidFormat`] for truncation, trailing bytes, magic/checksum
/// damage; [`ErrorCode::UnsupportedFormat`] for another version; or
/// [`ErrorCode::LimitExceeded`] for an oversized declared payload.
pub fn decode_frame(frame: &[u8]) -> Result<&[u8]> {
    if frame.len() < HEADER + DIGEST || &frame[..4] != b"AXF1" {
        return Err(invalid());
    }
    let len = inspect_frame_header(&frame[..HEADER])? - FRAME_OVERHEAD;
    if frame.len() != HEADER + len + DIGEST {
        return Err(invalid());
    }
    let end = HEADER + len;
    if Sha256::digest(&frame[..end])[..] != frame[end..] {
        return Err(invalid());
    }
    Ok(&frame[HEADER..end])
}

/// Validate an exact AXF1 header before reserving or allocating a body.
/// Returns total wire bytes (header, payload, digest); does not validate a digest.
///
/// # Errors
/// Rejects malformed header size/magic, unsupported version, and payloads over 1 MiB.
pub fn inspect_frame_header(header: &[u8]) -> Result<usize> {
    if header.len() != HEADER || &header[..4] != b"AXF1" {
        return Err(invalid());
    }
    if header[4..6] != [1, 0] {
        return Err(Error::new(
            ErrorCode::UnsupportedFormat,
            "unsupported header version",
        ));
    }
    let len = u32::from_le_bytes([header[6], header[7], header[8], header[9]]) as usize;
    if len > MAX_FRAME_PAYLOAD {
        return Err(Error::new(
            ErrorCode::LimitExceeded,
            "header length exceeds budget",
        ));
    }
    HEADER
        .checked_add(len)
        .and_then(|n| n.checked_add(DIGEST))
        .ok_or_else(invalid)
}

/// Encode AXF1 into an exactly sized caller-owned buffer without allocation.
/// The caller owns quota/lifetime; output never aliases the borrowed payload.
///
/// # Errors
/// Invalid size/oversize payload is rejected before changing output.
pub fn encode_frame_into(payload: &[u8], output: &mut [u8]) -> Result<()> {
    if payload.len() > MAX_FRAME_PAYLOAD {
        return Err(Error::new(
            ErrorCode::LimitExceeded,
            "frame payload exceeds 1 MiB",
        ));
    }
    if output.len() != payload.len() + FRAME_OVERHEAD {
        return Err(invalid());
    }
    output[..4].copy_from_slice(b"AXF1");
    output[4..6].copy_from_slice(&1_u16.to_le_bytes());
    output[6..10].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    let end = HEADER + payload.len();
    output[HEADER..end].copy_from_slice(payload);
    let digest = Sha256::digest(&output[..end]);
    output[end..].copy_from_slice(&digest);
    Ok(())
}
