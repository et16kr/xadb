//! Sorted immutable row references, with charges owned by each unique allocation.
use crate::{Error, ErrorCode, Result};
use std::mem::size_of;
use xadb_foundation::runtime::{Quota, Reservation};
use xadb_foundation::value::Value;

pub(crate) const ARC_HEADER_BYTES: usize = 2 * size_of::<usize>();

pub(crate) struct Charge {
    _base: Reservation,
    _extra_capacity: Option<Reservation>,
}

pub(crate) fn reserve(quota: &Quota, bytes: usize) -> Result<Reservation> {
    quota
        .reserve(
            u64::try_from(bytes)
                .map_err(|_| Error::new(ErrorCode::LimitExceeded, "allocation size"))?,
        )
        .map_err(|_| Error::new(ErrorCode::QuotaExceeded, "resident budget"))
}

/// Reserve accounting before allocation, then account any allocator capacity increase.
pub(crate) fn array<T>(quota: &Quota, count: usize, fixed: usize) -> Result<(Vec<T>, Charge)> {
    let requested = count
        .checked_mul(size_of::<T>())
        .and_then(|n| n.checked_add(fixed))
        .ok_or(Error::new(ErrorCode::LimitExceeded, "array size overflow"))?;
    let base = reserve(quota, requested)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| Error::new(ErrorCode::QuotaExceeded, "array allocation"))?;
    let extra = values
        .capacity()
        .saturating_sub(count)
        .checked_mul(size_of::<T>())
        .ok_or(Error::new(
            ErrorCode::LimitExceeded,
            "array capacity overflow",
        ))?;
    let extra_capacity = if extra == 0 {
        None
    } else {
        Some(reserve(quota, extra)?)
    };
    Ok((
        values,
        Charge {
            _base: base,
            _extra_capacity: extra_capacity,
        },
    ))
}

pub(crate) struct StoredRow {
    pub values: Vec<Value>,
    pub canonical_bytes: usize,
    _charge: Reservation,
}
impl StoredRow {
    pub fn new(values: Vec<Value>, canonical_bytes: usize, quota: &Quota) -> Result<Self> {
        let mut bytes = size_of::<Self>() + ARC_HEADER_BYTES;
        bytes = bytes
            .checked_add(
                values
                    .capacity()
                    .checked_mul(size_of::<Value>())
                    .ok_or(Error::new(ErrorCode::LimitExceeded, "row capacity"))?,
            )
            .ok_or(Error::new(ErrorCode::LimitExceeded, "row allocation"))?;
        for value in &values {
            let payload = match value {
                Value::Utf8(s) => s.capacity(),
                Value::Bytes(b) => b.capacity(),
                _ => 0,
            };
            bytes = bytes
                .checked_add(payload)
                .ok_or(Error::new(ErrorCode::LimitExceeded, "row payload capacity"))?;
        }
        Ok(Self {
            _charge: reserve(quota, bytes)?,
            values,
            canonical_bytes,
        })
    }
}
