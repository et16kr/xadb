//! Minimal values and checked arithmetic; SQL coercion/collation are not implemented.
//! Independent vectors in `tests/contracts.rs` cover NULL, type, size, and integer boundaries.
use crate::{Error, ErrorCode, Result};
use std::cmp::Ordering;

/// Maximum UTF-8 or binary payload size, measured in bytes.
pub const MAX_VALUE_BYTES: usize = 65_536;

/// Variant order defines index order; strings use binary UTF-8, without normalization.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Value {
    /// SQL-like NULL; [`Self::compare`] returns UNKNOWN, unlike the derived index ordering.
    Null,
    /// Boolean ordered false before true for index keys.
    Bool(bool),
    /// Signed 64-bit integer.
    Int64(i64),
    /// Owned UTF-8 bytes without normalization; callers must validate size.
    Utf8(String),
    /// Owned binary bytes; callers must validate size.
    Bytes(Vec<u8>),
}

impl Value {
    /// Validate variable-length payload size without allocation or mutation.
    ///
    /// # Errors
    /// Returns [`ErrorCode::LimitExceeded`] above [`MAX_VALUE_BYTES`].
    pub fn validate(&self) -> Result<()> {
        let size = match self {
            Self::Utf8(s) => s.len(),
            Self::Bytes(b) => b.len(),
            _ => 0,
        };
        if size > MAX_VALUE_BYTES {
            Err(Error::new(ErrorCode::LimitExceeded, "value exceeds 64 KiB"))
        } else {
            Ok(())
        }
    }

    /// NULL yields UNKNOWN (None); unlike index ordering, different types do not coerce.
    /// Validation occurs before NULL handling; input values are never changed.
    ///
    /// # Errors
    /// Returns [`ErrorCode::LimitExceeded`] for oversized values or
    /// [`ErrorCode::TypeMismatch`] for differing non-NULL types.
    pub fn compare(&self, other: &Self) -> Result<Option<Ordering>> {
        self.validate()?;
        other.validate()?;
        if matches!(self, Self::Null) || matches!(other, Self::Null) {
            return Ok(None);
        }
        if std::mem::discriminant(self) != std::mem::discriminant(other) {
            return Err(Error::new(
                ErrorCode::TypeMismatch,
                "implicit coercion unsupported",
            ));
        }
        Ok(Some(self.cmp(other)))
    }
}

/// Add signed integers without wrapping or saturation.
///
/// # Errors
/// Returns [`ErrorCode::Overflow`] when the mathematical sum is outside i64.
pub fn checked_add(left: i64, right: i64) -> Result<i64> {
    left.checked_add(right)
        .ok_or(Error::new(ErrorCode::Overflow, "i64 addition overflow"))
}
