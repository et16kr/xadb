//! Temporary bridge for the original allocation/empty/scope1 producers.
//! The sealed proof moves into the grant; there is no raw-field constructor.
use super::super::super::dirty_allocation::InstallPermit;
use super::{InstallGrant, InstallKind};

pub(in crate::storage::wal_io) fn from_legacy<'i, 'c, 's>(
    proof: InstallPermit<'i, 'c, 's>,
) -> InstallGrant<'i, 'c, 's> {
    InstallGrant {
        kind: InstallKind::Legacy(proof),
    }
}

/// Consumed argument plus enum construction/return carriers and local borrows.
/// No independent owner or fallible result exists at this conversion.
pub(in crate::storage::wal_io) const fn conversion_source_bytes() -> usize {
    use std::mem::size_of;
    size_of::<InstallPermit<'_, '_, '_>>()
        + 2 * size_of::<InstallGrant<'_, '_, '_>>()
        + 4 * size_of::<&()>()
}
