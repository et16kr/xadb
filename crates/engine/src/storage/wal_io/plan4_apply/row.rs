//! C0 consuming Row boundary of the original disk session.
//!
//! These wrappers fix the borrow, lifetime and ownership shape that the A and B
//! product bodies call. `MutationPort::preflight_row` runs the actual
//! preflight (S2-b1, `#row-write-validate-once`),
//! `PreflightRow::validation_lane` and `PreflightRow::reserve` the actual lane
//! and RowReserve (S2-b2), `ReservedRow::producer_lane`,
//! `ReservedRow::prepare` and `PreparedRow::apply` the actual Forward path
//! (S2-c), and `ReservedRow::cancel` the actual Cancel of that reservation
//! (S2-d). `MutationPort::commit_row` uses the original durable Commit barrier
//! and checked capsule cleanup (S3). Live statement/full rollback consumes the
//! original retained head chain and CLR/StatementFinish/AbortEnd receipts (S4).
//! Row reads borrow the original source and capture references under its latch
//! (S5; acceptance evidence is recorded separately). A
//! `ReservedRow` dropped while its reservation is active fences the session
//! (S2W-R23(2)); a driver that stops after a successful reserve without a
//! Forward ends it with `cancel`, which discharges the reservation without
//! the fence. No constructor accepts a raw TxnRef/UndoRef/RowId/q, and no
//! value here is an install, terminal or READY receipt.
#![allow(dead_code, reason = "Row consumers precede product SQL integration")]

use super::*;

#[path = "row/read.rs"]
mod read;
#[path = "row/recovery.rs"]
mod recovery;
#[path = "row/rollback.rs"]
mod rollback;
#[path = "row/write.rs"]
mod write;

pub(crate) use read::{
    MissingRowEvidence, RowCandidate, RowCopyObservation, RowReadOperation, RowSearchPage,
};
pub(crate) use recovery::{PreparedRecoveredClr, RecoveredClr, RecoveredClrStep};
#[cfg(test)]
pub(in crate::storage) use recovery::{recovered_clr_bytes, recovered_row_census};
pub(crate) use rollback::{
    PreparedRollbackTask, RollbackScope, RollbackStep, RowRollback, RowRollbackDone,
    RowRollbackTask,
};
pub(crate) use write::{
    PreflightRow, PreparedRow, ROW_PREFLIGHT_CALLER_BYTES, ROW_WRITE_CALLER_BYTES, ReservedRow,
    RowApplied, RowCancelled, RowPreflightStep, row_write_charge,
};

fn unsupported() -> RetentionError {
    unchanged(RetentionPhase::Prepare, RetentionCause::Unsupported)
}

/// A table identity checked against the actual Catalog source. Only the
/// Catalog `finish_table` adapter may issue it; a decoded TableSpec, ObjectId
/// or handle cannot. `table` carries the object generation
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
impl<'c, 's> CheckedTable<'c, 's> {
    /// The sole constructor. Its witness has a field private to the statement
    /// call, so only `CatalogStatementCall::finish_table` reaches this, after
    /// its own actual Found Table, Published epoch slot and registered read bit.
    pub(super) fn issue(
        _witness: super::read::TableIssue,
        core: &'c PageConcurrency<'s>,
        binding: &'s AtomicU8,
        fence: &'s SessionFence,
        table: ObjectHandle,
        slot: u8,
        anchor: PageIdentity,
    ) -> Self {
        Self {
            core,
            binding,
            fence,
            table,
            slot,
            anchor,
        }
    }
}
#[cfg(test)]
impl CheckedTable<'_, '_> {
    /// Scalar observation only; it cannot construct or refresh a handle.
    pub(in crate::storage) fn test_identity(&self) -> (ObjectHandle, u8, PageIdentity) {
        (self.table, self.slot, self.anchor)
    }
    pub(in crate::storage) fn test_same_origin(
        &self,
        core: &PageConcurrency<'_>,
        fence: &SessionFence,
    ) -> bool {
        std::ptr::eq(
            std::ptr::from_ref(self.core).cast::<()>(),
            std::ptr::from_ref(core).cast::<()>(),
        ) && std::ptr::eq(self.fence, fence)
            && self.core.uses_fence(self.fence)
    }
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
    pub(super) actor: TxnRef,
    pub(super) q: u64,
}
impl RowCommitObservation {
    pub(in crate::storage) fn actor(&self) -> TxnRef {
        self.actor
    }
    pub(in crate::storage) fn q(&self) -> u64 {
        self.q
    }
}

// Scalar cuts recorded by the actual adapters (S2-0 Row write amendment §4):
// scalars and 32-byte SHA-256 digests only. Every multi-page identity/stamp
// set is folded into one commitment that the next effect step recomputes
// from the current pool, WAL prefix, control ledger and the private before
// images (which A only ever borrows immutably); no raw stamp array is kept.
// They never borrow the private loan, so a wrapper does not refer to its own
// bytes. The A driver holds wrapper values several times, so each cut is
// bounded.
//
// The S2-0b amendment (Row write validate-once) runs `history::validate_live`
// once per row, in preflight. Reserve, prepare and cancel open no replay
// scope and inherit that proof through the two `source` commitments (the
// digest chain). Each `source` is a `ContentHasher` SHA-256 over, in order:
// 1. the domain tag `XRWC` (4 bytes), version 1 (1 byte) and the kind
//    (1 byte: 1 preflight source, 2 draft source);
// 2. the fields its doc lists, in that doc's order;
// 3. the page digest (32 bytes) of each target of T, in before order;
// 4. the `ControlCut` fields in declaration order: the bank, jobs and
//    inventory digests, issued and adopted.
// A difference is Before(Stale) on the same wrapper, before any effect. The
// lanes do not check the chain: they compute no page or control digest, and
// a change before or after a lane issue is refused by the next effect step's
// recompute. The cut sizes do not change.

/// Pack of `RowSourceRole` → before index, one nibble per role in role order
/// (0xF absent), and the physical before count (≤ 9) in nibble 9.
pub(super) type RowRoleMap = u64;

/// Recorded by preflight after residency, Free Undo selection, the row's one
/// `history::validate_live`, the typed read of T compared with the replay
/// arena, the WAL prefix recheck and read-set registration. The typed read's
/// pins are released before the prefix recheck and the registration, since
/// the isolation leaf is taken with no other guard held
/// (`record_table_read`); the gate, the RowWrite admission and the lease stay
/// held throughout, so no page, WAL or control change can come between the
/// read and the registration. Read by `validation_lane`, `reserve` and
/// `finish`; only `reserve` checks it.
///
/// No field records the registration: the cut is written only after the read
/// bit and catalog flag are registered, and every refusal and FlushRequired
/// returns before that, so every PreflightRow has registered both.
///
/// `validation_lane` has only the `source` commitment for the Catalog leaf
/// stamp and cell digest, the schema cell, the leaf expectation and the index
/// count, so it rebuilds `RowPreflightContext` from the current frames and the
/// private copies. It does not check the result against `source`: it computes
/// no page or control digest, and `reserve`'s commitment recompute and
/// candidate checks refuse any change before an effect (S2W-R04).
///
/// S2-b decision (A7): the cut keeps exactly these five fields and adds no
/// scalar for those values. Preflight leaves the whole Catalog leaf in the
/// SCHEMA range and T's before images in the BEFORE range, so the lane
/// rebuilds the schema cell and index count from that copy, the leaf
/// expectation from `roles` and the RowLeaf before header, and re-observes the
/// Catalog leaf stamp under the same gate and lease. A scalar would only
/// repeat what the copy and the commitment already fix, while the A driver
/// holds each cut several times on its narrowest path.
pub(super) struct RowPreflightCut {
    /// SHA-256 (kind 1, in the order the comment before the cuts gives) over
    /// the WAL prefix, the physical write target set T in before order
    /// (identity, kind, level and FrameStamp of each), the role map, the
    /// selected Free Undo container (allocator entry generation, page
    /// incarnation, slot0 Free; the first two are the same value, folded once
    /// each), the RootJob slot, the Catalog leaf stamp and
    /// schema cell digest, and the RowLeaf free bytes; then the page digests
    /// of T, taken from the before copies that were compared with the replay
    /// arena, and the `ControlCut` fields. `reserve` recomputes it over a new
    /// typed read of T into `before`, with no replay; any difference is
    /// Before/Stale and the row is prepared again from `validation_lane`
    /// (S2W-R05, S2W-R07).
    pub(super) source: [u8; 32],
    /// WAL complete LSN of the commitment: the RowReserve frame's expected
    /// previous LSN and a cheap stale check before rehashing.
    pub(super) complete_lsn: u64,
    /// Where each logical source role's before image is; the identities are
    /// read back from those images' headers.
    pub(super) roles: RowRoleMap,
    /// RowLeaf free bytes reported by the validation lane; `reserve` refuses a
    /// candidate that does not fit them (Capacity).
    pub(super) leaf_free: u16,
    /// The Jobs ordinal of the RootJob slot preflight chose for RowReserve.
    pub(super) root_job: u8,
}

/// Recorded by RowReserve's completed install (Forward) or by the rollback
/// selection (CLR); carried by ReservedRow and RowRollbackTask and checked by
/// `prepare`, `apply` and `cancel`. `producer_lane` reads it but does not
/// check the chain.
pub(super) struct RowDraftCut {
    /// SHA-256 (kind 2, in the order the comment before the cuts gives) over
    /// the WAL prefix after the last installed group, T in before order with
    /// its current stamps, the role map, the issued identity (actor, RootJob,
    /// RowId, UndoRef/sequence, planned mutation), the actor/Aging descriptor
    /// bytes and the Jobs rows the producer lane borrows; then the page
    /// digests of T and the `ControlCut` fields. The page digests come from a
    /// typed read of the pool pages after the install, under the gate and the
    /// lease, not from the after images in the private loan, so `prepare` and
    /// `cancel`, which read the pool, compare like with like; the same read
    /// refreshes `before` for the next step. `prepare` and `cancel` recompute
    /// it with no replay, and so does `apply` if its stage-time recheck
    /// recomputes it. A difference is Before/Stale on the same wrapper
    /// (S2W-R07, S2W-R21 first hook).
    pub(super) source: [u8; 32],
    /// The sealed input: canonical row digest (Forward) or current row
    /// projection (CLR). The observer compares A's frame against it.
    pub(super) input: [u8; 32],
    /// WAL complete LSN after the last installed group: the Forward/CLR
    /// frame's expected previous LSN.
    pub(super) complete_lsn: u64,
    /// Logical mutation number of the row version this step writes.
    pub(super) planned_mutation: u64,
    pub(super) roles: RowRoleMap,
    /// Canonical length (Forward) or current row length (CLR).
    pub(super) input_len: u16,
    /// 0 Forward, 1 CLR (`RowOperation`).
    pub(super) operation: u8,
}

/// Recorded by `prepare` after the observer independently recomputed and
/// compared the whole declared frame and every after page; checked by `apply`
/// before the stage. The source cut it was checked against is the draft's,
/// held by the same reservation.
pub(super) struct RowPreparedCut {
    /// SHA-256 of the checked frame bytes `[0, frame_len)`.
    pub(super) frame: [u8; 32],
    /// SHA-256 over the after count, roles and every checked after page in
    /// physical order, then the XOI successor page `prepare` encoded beside
    /// them (S2-c): `apply` reassembles the group from the loan without the
    /// observer or the inventory projection, so this digest covers every
    /// byte the install takes from the loan besides the frame.
    pub(super) after: [u8; 32],
    /// LSN the frame header claims; the stage requires it to be next.
    pub(super) lsn: u64,
    /// Checked frame length: the WAL bytes the stage consumes.
    pub(super) frame_len: u32,
    /// `RowPageRole` of each after page in physical order, three bits each.
    pub(super) after_roles: u16,
    /// Changed pages (≤ 5).
    pub(super) after_count: u8,
    /// 0 Forward, 1 CLR (`RowOperation`).
    pub(super) operation: u8,
}

/// Most bytes of any one cut (S2-0 Row write amendment §4, S2W-C05).
const ROW_CUT_MAX_BYTES: usize = 128;
const _: () = {
    use std::mem::size_of;
    assert!(size_of::<RowPreflightCut>() <= ROW_CUT_MAX_BYTES);
    assert!(size_of::<RowDraftCut>() <= ROW_CUT_MAX_BYTES);
    assert!(size_of::<RowPreparedCut>() <= ROW_CUT_MAX_BYTES);
    // Nine roles and the count fit the role map.
    assert!(10 * 4 <= RowRoleMap::BITS as usize);
};

#[cfg(test)]
#[path = "row/census_tests.rs"]
mod census_tests;

#[cfg(test)]
#[path = "row/v09_census_tests.rs"]
mod v09_census_tests;
