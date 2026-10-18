//! Nonzero logical IDs with checked progression, never pointers or ID allocators.
//! Object allocation, global uniqueness, persistence, and snapshot pinning belong to DEV2.
//! See `identities_reject_zero_wrap_and_reused_or_changed_objects` in `tests/contracts.rs`.
use crate::{Error, ErrorCode, Result};
use std::num::NonZeroU64;

macro_rules! identifier {
    ($($name:ident),+ $(,)?) => {$ (
        #[doc = concat!("Nonzero logical ", stringify!($name), "; construction does not allocate a unique identity.")]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(NonZeroU64);
        impl $name {
            /// Validate a nonzero numeric identity.
            ///
            /// # Errors
            /// Returns [`ErrorCode::InvalidIdentity`] for zero, without effects.
            pub fn new(value: u64) -> Result<Self> {
                NonZeroU64::new(value).map(Self).ok_or(Error::new(
                    ErrorCode::InvalidIdentity, "zero identity is reserved"))
            }
            /// Read the numeric value without changing its identity or lifetime.
            pub const fn get(self) -> u64 { self.0.get() }
            /// Compute the next value; does not reserve or publish it.
            ///
            /// # Errors
            /// Returns [`ErrorCode::Overflow`] at u64::MAX; never wraps.
            pub fn next(self) -> Result<Self> {
                let value = self.get().checked_add(1).ok_or(Error::new(
                    ErrorCode::Overflow, "identity exhausted; wrapping forbidden"))?;
                Self::new(value)
            }
        }
    )+};
}
identifier!(ObjectId, RowId, Generation, SchemaEpoch, CommitSeq);

/// Logical identity, never an address. Generation changes on reuse, epoch on schema change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ObjectHandle {
    /// Object number; caller owns allocation and uniqueness.
    pub object: ObjectId,
    /// Reuse generation, distinct from schema changes.
    pub generation: Generation,
    /// Schema generation associated with this handle.
    pub schema_epoch: SchemaEpoch,
}

impl ObjectHandle {
    /// Compare all identity components with a caller-supplied current handle.
    /// No object lookup or pin is acquired; caller must stabilize the current identity.
    ///
    /// # Errors
    /// Returns [`ErrorCode::StaleHandle`] on any mismatch, without effects.
    pub fn validate_current(self, current: Self) -> Result<()> {
        if self == current {
            Ok(())
        } else {
            Err(Error::new(
                ErrorCode::StaleHandle,
                "object, generation or schema changed",
            ))
        }
    }
}
