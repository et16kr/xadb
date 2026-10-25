//! Canonical DEV3 composite/physical index keys, with no storage effects.
//! Byte contract: `devdoc/contracts/dev3-key-format.md`.
// Remove when the first tree/index consumer connects this representation leaf.
#![allow(dead_code)]

use foundation::{
    codec::{encode_key_into, encoded_key_len, inspect_key_prefix},
    identity::RowId,
    value::Value,
};
use std::cmp::Ordering;

pub(crate) const MAX_KEY_BYTES: usize = 1_024;
pub(crate) const MAX_KEY_COLUMNS: usize = 4;
pub(crate) const MAX_PHYSICAL_KEY_BYTES: usize = MAX_KEY_BYTES + 8;

/// Known errors without external effects. All encoder failures preserve output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyError {
    InvalidColumns,
    TooLarge,
    NoSpace,
    Malformed,
    InvalidRowIdentity,
}

/// Canonical bytes borrowing caller storage. Schema/unique locks and snapshot
/// visibility remain the owning index's responsibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CompositeKey<'a> {
    bytes: &'a [u8],
    columns: u8,
    nulls: u8,
}

impl<'a> CompositeKey<'a> {
    /// Validate every scalar, exact arity, and the escaped byte limit.
    pub(crate) fn decode(bytes: &'a [u8], columns: usize) -> Result<Self, KeyError> {
        validate_columns(columns)?;
        if bytes.len() > MAX_KEY_BYTES {
            return Err(KeyError::TooLarge);
        }
        let mut remaining = bytes;
        let mut nulls = 0;
        for _ in 0..columns {
            let prefix = inspect_key_prefix(remaining).map_err(|_| KeyError::Malformed)?;
            nulls += u8::from(prefix.is_null);
            remaining = &remaining[prefix.encoded_len..];
        }
        if !remaining.is_empty() {
            return Err(KeyError::Malformed);
        }
        Ok(Self {
            bytes,
            columns: columns as u8,
            nulls,
        })
    }

    pub(crate) fn as_bytes(self) -> &'a [u8] {
        self.bytes
    }

    pub(crate) fn has_null(self) -> bool {
        self.nulls != 0
    }

    pub(crate) fn all_null(self) -> bool {
        self.nulls == self.columns
    }
}

impl Ord for CompositeKey<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.bytes.cmp(other.bytes)
    }
}

impl PartialOrd for CompositeKey<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A validated composite key and logical RowId tie-breaker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PhysicalKey<'a> {
    composite: CompositeKey<'a>,
    row: RowId,
}

impl<'a> PhysicalKey<'a> {
    /// Decode the exact composite plus eight-byte nonzero RowId representation.
    pub(crate) fn decode(bytes: &'a [u8], columns: usize) -> Result<Self, KeyError> {
        let split = bytes.len().checked_sub(8).ok_or(KeyError::Malformed)?;
        let composite = CompositeKey::decode(&bytes[..split], columns)?;
        let mut row = [0; 8];
        row.copy_from_slice(&bytes[split..]);
        let row = RowId::new(u64::from_be_bytes(row)).map_err(|_| KeyError::InvalidRowIdentity)?;
        Ok(Self { composite, row })
    }

    pub(crate) fn composite(self) -> CompositeKey<'a> {
        self.composite
    }

    pub(crate) fn row(self) -> RowId {
        self.row
    }
}

impl Ord for PhysicalKey<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.composite
            .cmp(&other.composite)
            .then_with(|| self.row.cmp(&other.row))
    }
}

impl PartialOrd for PhysicalKey<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn validate_columns(columns: usize) -> Result<(), KeyError> {
    if !(1..=MAX_KEY_COLUMNS).contains(&columns) {
        return Err(KeyError::InvalidColumns);
    }
    Ok(())
}

fn layout(values: &[Value]) -> Result<([usize; MAX_KEY_COLUMNS], usize, u8), KeyError> {
    validate_columns(values.len())?;
    let mut lengths = [0; MAX_KEY_COLUMNS];
    let mut total = 0;
    let mut nulls = 0;
    for (index, value) in values.iter().enumerate() {
        let size = encoded_key_len(value).map_err(|_| KeyError::TooLarge)?;
        total += size;
        if total > MAX_KEY_BYTES {
            return Err(KeyError::TooLarge);
        }
        lengths[index] = size;
        nulls += u8::from(matches!(value, Value::Null));
    }
    Ok((lengths, total, nulls))
}

/// Encode all columns into the beginning of output and borrow the used prefix.
/// All validation precedes writing; errors and unused tail bytes are unchanged.
/// No allocation, lock, I/O, or index/schema publication occurs.
pub(crate) fn encode_composite<'a>(
    values: &[Value],
    output: &'a mut [u8],
) -> Result<CompositeKey<'a>, KeyError> {
    let (lengths, total, nulls) = layout(values)?;
    if output.len() < total {
        return Err(KeyError::NoSpace);
    }
    let mut offset = 0;
    for (value, length) in values.iter().zip(lengths) {
        // The exact lengths and every immutable input were validated above;
        // the scalar writer has no other failure conditions or allocation.
        encode_key_into(value, &mut output[offset..offset + length])
            .map_err(|_| KeyError::Malformed)?;
        offset += length;
    }
    Ok(CompositeKey {
        bytes: &output[..total],
        columns: values.len() as u8,
        nulls,
    })
}

/// Encode a physical key, prevalidating the RowId suffix capacity before writes.
/// The returned view borrows output; failures and unused tail remain unchanged.
pub(crate) fn encode_physical_key<'a>(
    values: &[Value],
    row: RowId,
    output: &'a mut [u8],
) -> Result<PhysicalKey<'a>, KeyError> {
    let (_, total, _) = layout(values)?;
    if output.len() < total + 8 {
        return Err(KeyError::NoSpace);
    }
    let (composite_output, tail) = output.split_at_mut(total);
    let composite = encode_composite(values, composite_output)?;
    tail[..8].copy_from_slice(&row.get().to_be_bytes());
    Ok(PhysicalKey { composite, row })
}

#[cfg(test)]
#[path = "key_tests.rs"]
mod tests;
