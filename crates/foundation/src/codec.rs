//! Bounded canonical keys and a current-build frame. This is not a WAL record format.
use crate::{Error, ErrorCode, Result, value::Value};
use sha2::{Digest, Sha256};

/// Maximum current-build frame payload in bytes, excluding header and digest.
pub const MAX_FRAME_PAYLOAD: usize = 1_048_576;
/// AXF1 header size, before payload and digest.
pub const FRAME_HEADER_BYTES: usize = 10;
const HEADER: usize = FRAME_HEADER_BYTES;
/// AXF1 envelope overhead, including the SHA-256 digest.
pub const FRAME_OVERHEAD: usize = 42;
const DIGEST: usize = 32;

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
    value.validate()?;
    let size = match value {
        Value::Utf8(v) => 3 + 2 * v.len(),
        Value::Bytes(v) => 3 + 2 * v.len(),
        _ => 9,
    };
    let mut key = allocate(size)?;
    let string = match value {
        Value::Null => {
            key.push(0);
            None
        }
        Value::Bool(v) => {
            key.extend_from_slice(&[1, u8::from(*v)]);
            None
        }
        Value::Int64(v) => {
            key.push(2);
            key.extend_from_slice(&((*v as u64) ^ (1_u64 << 63)).to_be_bytes());
            None
        }
        Value::Utf8(v) => {
            key.push(3);
            Some(v.as_bytes())
        }
        Value::Bytes(v) => {
            key.push(4);
            Some(v.as_slice())
        }
    };
    if let Some(bytes) = string {
        for b in bytes {
            key.push(*b);
            if *b == 0 {
                key.push(255);
            }
        }
        key.extend_from_slice(&[0, 0]);
    }
    Ok(key)
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
