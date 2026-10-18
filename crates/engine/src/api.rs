//! Logical values, identities and schema. No row address or persistent layout is exposed.
use crate::{Error, ErrorCode, Result};
use std::sync::Arc;
use xadb_foundation::identity::{ObjectHandle, ObjectId, RowId};

/// Database incarnation, distinct from timeline and open-instance qualification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DatabaseId([u8; 16]);
impl DatabaseId {
    /// Validate a nonzero identity. K0 fixtures supply it; this does not generate randomness.
    ///
    /// # Errors
    /// All-zero input is invalid and has no effects.
    pub fn new(bytes: [u8; 16]) -> Result<Self> {
        if bytes == [0; 16] {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "zero database identity",
            ));
        }
        Ok(Self(bytes))
    }
    /// Stable numeric bytes, never a pointer or authentication secret.
    pub const fn bytes(self) -> [u8; 16] {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Namespace {
    pub database: DatabaseId,
    pub timeline: u64,
}
pub(crate) struct Instance {
    pub namespace: Namespace,
    pub _charge: xadb_foundation::runtime::Reservation,
}
impl std::fmt::Debug for Instance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Instance")
            .field("namespace", &self.namespace)
            .finish_non_exhaustive()
    }
}

/// Owning capability for a logical table in one open instance.
/// Construct through lookup/create; after reopening, acquire a new handle.
#[derive(Clone, Debug)]
pub struct TableHandle {
    pub(crate) instance: Arc<Instance>,
    pub(crate) identity: ObjectHandle,
}
impl TableHandle {
    /// Logical metadata identity. It does not reconstruct a valid handle by itself.
    pub const fn identity(&self) -> ObjectHandle {
        self.identity
    }
}
impl PartialEq for TableHandle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.instance, &other.instance) && self.identity == other.identity
    }
}
impl Eq for TableHandle {}

/// Stable logical row identity including the database/timeline namespace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowUid {
    pub(crate) namespace: Namespace,
    pub(crate) table: ObjectHandle,
    pub(crate) row: RowId,
}
impl RowUid {
    /// Per-epoch logical row number; not an address.
    pub const fn row_id(self) -> RowId {
        self.row
    }
    /// Owning table number; generation is part of equality even though IDs are not reused.
    pub const fn table_id(self) -> ObjectId {
        self.table.object
    }
    /// Full logical table identity, including generation and schema epoch.
    /// These numbers do not reconstruct the open-instance table capability.
    pub const fn table_identity(self) -> ObjectHandle {
        self.table
    }
    /// Database incarnation for cross-DB identity comparisons.
    pub const fn database_id(self) -> DatabaseId {
        self.namespace.database
    }
    /// Restore branch namespace.
    pub const fn timeline(self) -> u64 {
        self.namespace.timeline
    }
}

/// Supported fixed-schema types. NULL is a value governed by nullability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataType {
    /// Boolean scalar.
    Bool,
    /// Signed 64-bit scalar without coercion.
    Int64,
    /// UTF-8 bytes without normalization.
    Utf8,
    /// Arbitrary bytes.
    Binary,
}
/// One fixed schema column; validation occurs before catalog publication.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Column {
    /// Byte-exact unique name, 1..=128 UTF-8 bytes with no NUL.
    pub name: String,
    /// Required non-NULL scalar type.
    pub data_type: DataType,
    /// Whether NULL is accepted.
    pub nullable: bool,
}
/// Nonpartitioned table schema for the volatile development substrate. ALTER/index/SQL semantics are not implemented.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableSpec {
    /// Byte-exact catalog name.
    pub name: String,
    /// Ordered schema of 1..=32 columns.
    pub columns: Vec<Column>,
}

/// Explicit substrate resource limits, not process RSS or product capacity guarantees.
#[derive(Clone, Copy, Debug)]
pub struct StoreOptions {
    /// Accounted unique committed and pinned-old allocations; default 64 MiB.
    pub resident_bytes: u64,
    /// Owning user snapshots, 1..=32; cursors/views share their source pin.
    pub snapshot_pins: u64,
}
impl Default for StoreOptions {
    fn default() -> Self {
        Self {
            resident_bytes: 64 * 1024 * 1024,
            snapshot_pins: 32,
        }
    }
}

/// Cumulative substrate work and current unique reservation usage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Metrics {
    /// Attempted catalog/row mutations, including rejected requests.
    pub operations: u64,
    /// Rows returned by successful read or scan calls.
    pub rows_read: u64,
    /// Existing row references cloned while preparing changed table arrays.
    pub row_references_copied: u64,
    /// Current charged unique allocation bytes, including pinned-old state/output.
    pub resident_bytes: u64,
    /// Active owning snapshot pins, shared by derived cursors and row views.
    pub snapshot_pins: u64,
}
