//! Sorted immutable row references, with charges owned by each unique allocation.
use crate::{Error, ErrorCode, Result};
use std::mem::size_of;
use xadb_foundation::runtime::{Quota, Reservation};
use xadb_foundation::value::Value;

pub(crate) const ARC_HEADER_BYTES: usize = 2 * size_of::<usize>();

pub(crate) struct Charge {
    base: AllocationCharge,
    _extra_capacity: Option<AllocationCharge>,
}
impl Charge {
    /// Adopt caller-prepared storage, charging its actual capacity before making
    /// the owning engine allocation. The caller accounts any preparation buffer.
    pub fn resident_array<T>(quota: &Quota, capacity: usize, fixed: usize) -> Result<Self> {
        let bytes = capacity
            .checked_mul(size_of::<T>())
            .and_then(|bytes| bytes.checked_add(fixed))
            .ok_or(Error::new(
                ErrorCode::LimitExceeded,
                "adopted array capacity",
            ))?;
        Ok(Self {
            base: AllocationBudget::resident(quota).reserve(bytes)?,
            _extra_capacity: None,
        })
    }
    pub fn is_private(&self) -> bool {
        self.base.is_private()
    }
}

/// A private allocation must fit both its transaction and the shared pool.
/// The first lease is returned automatically if the second reservation fails.
#[derive(Clone)]
pub(crate) struct AllocationBudget {
    primary: Quota,
    secondary: Option<Quota>,
}
pub(crate) struct AllocationCharge {
    _primary: Reservation,
    secondary: Option<Reservation>,
}
impl AllocationCharge {
    pub fn is_private(&self) -> bool {
        self.secondary.is_some()
    }
}
impl AllocationBudget {
    pub fn resident(quota: &Quota) -> Self {
        Self {
            primary: quota.clone(),
            secondary: None,
        }
    }
    pub fn private(transaction: Quota, shared: Quota) -> Self {
        Self {
            primary: transaction,
            secondary: Some(shared),
        }
    }
    pub fn used(&self) -> u64 {
        self.primary.used()
    }
    pub fn reserve(&self, bytes: usize) -> Result<AllocationCharge> {
        let primary = reserve(&self.primary, bytes)?;
        let secondary = self
            .secondary
            .as_ref()
            .map(|quota| reserve(quota, bytes))
            .transpose()?;
        Ok(AllocationCharge {
            _primary: primary,
            secondary,
        })
    }
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
    array_in(&AllocationBudget::resident(quota), count, fixed)
}

pub(crate) fn array_in<T>(
    budget: &AllocationBudget,
    count: usize,
    fixed: usize,
) -> Result<(Vec<T>, Charge)> {
    let requested = count
        .checked_mul(size_of::<T>())
        .and_then(|n| n.checked_add(fixed))
        .ok_or(Error::new(ErrorCode::LimitExceeded, "array size overflow"))?;
    let base = budget.reserve(requested)?;
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
        Some(budget.reserve(extra)?)
    };
    Ok((
        values,
        Charge {
            base,
            _extra_capacity: extra_capacity,
        },
    ))
}

pub(crate) struct StoredRow {
    pub values: Vec<Value>,
    pub canonical_bytes: usize,
    charge: Charge,
}
impl StoredRow {
    pub fn new(values: Vec<Value>, canonical_bytes: usize, quota: &Quota) -> Result<Self> {
        Self::new_in(values, canonical_bytes, &AllocationBudget::resident(quota))
    }
    pub fn new_in(
        values: Vec<Value>,
        canonical_bytes: usize,
        budget: &AllocationBudget,
    ) -> Result<Self> {
        Ok(Self {
            charge: Charge {
                base: budget.reserve(row_allocation_bytes(&values, values.capacity(), true)?)?,
                _extra_capacity: None,
            },
            values,
            canonical_bytes,
        })
    }
    pub fn is_private(&self) -> bool {
        self.charge.is_private()
    }

    /// Copy private payload into the resident ledger before publication. Existing
    /// resident rows are shared by the caller and never pass through this method.
    pub fn to_resident(&self, quota: &Quota) -> Result<Self> {
        let budget = AllocationBudget::resident(quota);
        let requested = row_allocation_bytes(&self.values, self.values.len(), false)?;
        let base = budget.reserve(requested)?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(self.values.len())
            .map_err(|_| Error::new(ErrorCode::QuotaExceeded, "resident row value allocation"))?;
        for value in &self.values {
            values.push(match value {
                Value::Null => Value::Null,
                Value::Bool(value) => Value::Bool(*value),
                Value::Int64(value) => Value::Int64(*value),
                Value::Utf8(value) => {
                    let mut text = String::new();
                    text.try_reserve_exact(value.len()).map_err(|_| {
                        Error::new(ErrorCode::QuotaExceeded, "resident text allocation")
                    })?;
                    text.push_str(value);
                    Value::Utf8(text)
                }
                Value::Bytes(value) => {
                    let mut bytes = Vec::new();
                    bytes.try_reserve_exact(value.len()).map_err(|_| {
                        Error::new(ErrorCode::QuotaExceeded, "resident bytes allocation")
                    })?;
                    bytes.extend_from_slice(value);
                    Value::Bytes(bytes)
                }
            });
        }
        let extra =
            row_allocation_bytes(&values, values.capacity(), true)?.saturating_sub(requested);
        let extra_capacity = if extra == 0 {
            None
        } else {
            Some(budget.reserve(extra)?)
        };
        Ok(Self {
            values,
            canonical_bytes: self.canonical_bytes,
            charge: Charge {
                base,
                _extra_capacity: extra_capacity,
            },
        })
    }
}

fn row_allocation_bytes(values: &[Value], count: usize, capacity: bool) -> Result<usize> {
    let mut bytes = count
        .checked_mul(size_of::<Value>())
        .and_then(|bytes| bytes.checked_add(size_of::<StoredRow>() + ARC_HEADER_BYTES))
        .ok_or(Error::new(ErrorCode::LimitExceeded, "row allocation"))?;
    for value in values {
        let payload = match value {
            Value::Utf8(value) => {
                if capacity {
                    value.capacity()
                } else {
                    value.len()
                }
            }
            Value::Bytes(value) => {
                if capacity {
                    value.capacity()
                } else {
                    value.len()
                }
            }
            _ => 0,
        };
        bytes = bytes
            .checked_add(payload)
            .ok_or(Error::new(ErrorCode::LimitExceeded, "row payload capacity"))?;
    }
    Ok(bytes)
}
