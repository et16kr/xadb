//! C0 consuming Row boundary of the original disk session.
//!
//! These wrappers fix the borrow, lifetime and ownership shape that the A and B
//! product bodies call. Their factories return an effect-free `Unsupported`
//! before any source guard, pin, admission, WAL or page effect: the actual
//! preflight/RowReserve, Row read source, rollback and Commit adapters are later
//! C2/C3 supplies. No constructor accepts a raw TxnRef/UndoRef/RowId/q, and no
//! value here is an install, terminal or READY receipt.
#![allow(
    dead_code,
    reason = "actual Row source, reservation and rollback adapters are not supplied yet"
)]

use super::*;

#[path = "row/read.rs"]
mod read;
#[path = "row/rollback.rs"]
mod rollback;
#[path = "row/write.rs"]
mod write;

pub(crate) use read::{
    MissingRowEvidence, RowCandidate, RowCopyObservation, RowReadOperation, RowSearchPage,
};
pub(crate) use rollback::{
    PreparedRollbackTask, RollbackScope, RollbackStep, RowRollback, RowRollbackDone,
    RowRollbackTask,
};
pub(crate) use write::{PreflightRow, PreparedRow, ReservedRow, RowApplied, RowCancelled};

fn unsupported() -> RetentionError {
    unchanged(RetentionPhase::Prepare, RetentionCause::Unsupported)
}

/// A table identity checked against the actual Catalog source. Only the
/// Catalog `finish_table` adapter may issue it (not supplied yet); a decoded
/// TableSpec, ObjectId or handle cannot. `table` carries the object generation
/// and SchemaEpoch, `slot` the checked table slot of the original epoch. Row C
/// rechecks the same open, schema, table generation, slot and anchor at every
/// use, so holding this value is not a pin or write grant.
pub(crate) struct CheckedTable<'c, 's> {
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    fence: &'s SessionFence,
    table: ObjectHandle,
    slot: u8,
    anchor: PageIdentity,
}

#[allow(
    clippy::large_enum_variant,
    reason = "the same consuming wrapper is returned inline for retry or cancel"
)]
pub(crate) enum RowOwnedError<T> {
    /// Rejected before the step's first effect; the wrapper keeps its proof.
    Before { error: RetentionError, owned: T },
    /// Original fence and remaining obligations stay with the owner.
    Fatal(RetentionError),
}

/// Scalar result of an applied Forward or CLR; neither a receipt nor a pin.
#[derive(Debug)]
pub(crate) struct RowMutationObservation {
    operation: RowOperation,
    row: RowId,
    undo: UndoRef,
}
impl RowMutationObservation {
    pub(in crate::storage) fn operation(&self) -> RowOperation {
        self.operation
    }
    pub(in crate::storage) fn row(&self) -> RowId {
        self.row
    }
    pub(in crate::storage) fn undo(&self) -> UndoRef {
        self.undo
    }
}

/// Commit observation after epoch and Q publication; not a WAL receipt.
#[derive(Debug)]
pub(crate) struct RowCommitObservation {
    actor: TxnRef,
    q: u64,
}
impl RowCommitObservation {
    pub(in crate::storage) fn actor(&self) -> TxnRef {
        self.actor
    }
    pub(in crate::storage) fn q(&self) -> u64 {
        self.q
    }
}

// Scalar cuts recorded by the actual adapters. They never borrow the private
// loan, so a wrapper does not refer to its own bytes.
struct RowPreflightCut {
    _pending: (),
}
struct RowDraftCut {
    _pending: (),
}
struct RowPreparedCut {
    _pending: (),
}
