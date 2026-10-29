//! C-owned scope3 Row group candidates and the whole-group observer. Every
//! input is a borrowed stable observation. Nothing here selects whether a
//! transaction is Row or Catalog scope, admits a source, pins, stages,
//! installs, records history or grants any recovery authority. A's Forward and
//! CLR are observed only: their producer is `row::insert::group`.
//!
//! Contract readings (dev3-module-consumers.md), each enforced by `observe`:
//! - Begin leases the first Free Job from the Root cursor with the base
//!   control 1×3040; every Reserve holds 3×3040 (:178, :270); StatementFinish
//!   returns the Active actor to the 1×3040 base.
//! - Reserve issues `row_high+1`, `undo_high+1`, `last_mutation+1` and the
//!   next generation of the planned Free U (:71, :75-76); Cancel returns none
//!   of them (:79).
//! - ReleaseRole/RootFree need the source actor's TerminalReady Job; RootFree
//!   and TerminalFinish keep the Job/U generations (:236-238).
//! - Prune takes only the actor's current head after Commit (:214, :242) and
//!   re-encodes the record with the caller's stable table schema.
//! - RowReserve checks the table/anchor/leaf proof (:73): the stable RowLeaf
//!   types every row, lacks the issued RowId and takes the planned cell
//!   without a split (leaf space fails before the first Reserve effect, :282).
//! - Aging selection order is the caller's; a non-handoff group with a Job
//!   writes only the Root cursor `(ordinal+1) % 64`.
//! - Begin's Row/Catalog scope is the caller's explicit input; Catalog is
//!   Unsupported here.
use super::{Error, Operation, Result, status::StatementBoundary};
use crate::storage::{
    page::{PAGE_BYTES, PageIdentity, PageKind, PageRef},
    retention_page::{JOB_BYTES, JOB_COUNT, ROOT_BYTES, row::JobScope},
    row_record::RowSchemaInput,
    status_home::StatusHome,
    transaction_record::{StatusSlotRef, TxnRef},
    undo_record::UNDO_RECORD_BYTES,
    undo_ref::UndoRef,
    wal_frame::{FrameExpectation, WalFrameHeader},
};
use foundation::identity::{ObjectHandle, RowId};

mod cells;
mod frame;
mod graph;
mod prepare;
mod source;

/// Fixed output roles of the caller's five-page loan. Physical WAL order is
/// computed separately from the actual page identities.
pub(crate) const ROOT: usize = 0;
pub(crate) const JOB: usize = 1;
pub(crate) const STATUS: usize = 2;
pub(crate) const UNDO: usize = 3;
pub(crate) const LEAF: usize = 4;
/// Largest Row tree cell C (`180+2048`) and its XRW1 record.
const MAX_CELL: usize = super::MAX_CELL;
const MAX_RECORD: usize = MAX_CELL - 12;
/// Row Job census aggregate (`retention_page::row::summarize_jobs`).
const AGGREGATE_BYTES: usize = 640;

/// Backing of one group's large prepared values, in this order: both
/// descriptors, core, Row cell, Job, Root, U record, Edge/Count/Delta and the
/// Job census aggregate. Neither descriptor is written before prepare's
/// descriptor step, so Reserve/Prune borrow their first `MAX_RECORD` bytes as
/// record scratch and clear them before it (`prepare::record_scratch`).
/// `observe_borrowed` takes this from the replay WORK_END zero tail after
/// provenance, the lane the Catalog arm borrows exclusively
/// (plan4_retention/history.rs:42-46); produce/observe use a local array.
pub(crate) const ROW_REPLAY_SCRATCH_BYTES: usize = 2 * 1536
    + 256
    + MAX_CELL
    + JOB_BYTES
    + ROOT_BYTES
    + UNDO_RECORD_BYTES
    + graph::AUX_BYTES
    + AGGREGATE_BYTES;
/// The WORK_END tail after provenance: two pages less 64 Claim352.
const _: () = assert!(ROW_REPLAY_SCRATCH_BYTES <= 2 * PAGE_BYTES - 64 * 352);
const _: () = assert!(MAX_RECORD <= 2 * 1536);

/// Plan of one RowReserve. RowId, UndoSequence, mutation, slot and Job
/// generations are issued from the stable Root/descriptor/page, never here.
#[derive(Clone, Copy)]
pub(crate) struct ReservePlan<'a> {
    /// Ordinal of the actor's TerminalLeased Job; actor and Status come from it.
    pub(crate) terminal: usize,
    /// Free Undo container chosen by the original allocator/provisioning owner.
    pub(crate) container: PageRef,
    pub(crate) table: ObjectHandle,
    /// The caller's live statement sequence (nonzero, non-decreasing).
    pub(crate) statement: u64,
    /// A's validated canonical inline row value (L bytes).
    pub(crate) canonical: &'a [u8],
}

/// Stable table observation: anchor proof, current RowLeaf and schema. This is
/// not a CheckedTable, epoch or Catalog selection token.
#[derive(Clone, Copy)]
pub(crate) struct TableSource<'a> {
    pub(crate) handle: ObjectHandle,
    pub(crate) anchor: PageRef,
    pub(crate) anchor_image: &'a [u8; PAGE_BYTES],
    pub(crate) leaf_image: Option<&'a [u8; PAGE_BYTES]>,
    pub(crate) schema: RowSchemaInput<'a>,
}

#[derive(Clone, Copy)]
pub(crate) enum Change<'a> {
    AgingStart,
    /// The scope is the caller's explicit decision; only Row is produced here.
    Begin {
        actor: TxnRef,
        status: StatusSlotRef,
        scope: JobScope,
    },
    #[allow(
        dead_code,
        reason = "replay refuses RowReserve (no canonical value in its frame); its producer caller is the RowReserve entry"
    )]
    Reserve(&'a ReservePlan<'a>),
    /// Observe only; A produces Forward.
    Forward {
        ordinal: usize,
        statement: StatementBoundary,
    },
    Cancel {
        ordinal: usize,
    },
    /// `None` is a full transaction rollback, `Some` a statement rollback.
    RollbackStart {
        terminal: usize,
        statement: Option<StatementBoundary>,
    },
    /// Observe only; A produces CLR.
    Clr {
        ordinal: usize,
        statement: Option<StatementBoundary>,
    },
    StatementFinish {
        terminal: usize,
        statement: StatementBoundary,
    },
    Commit {
        terminal: usize,
    },
    AbortEnd {
        terminal: usize,
    },
    Prune {
        ordinal: usize,
    },
    ReleaseRole {
        ordinal: usize,
    },
    RootFree {
        ordinal: usize,
    },
    TerminalFinish {
        ordinal: usize,
    },
}
impl Change<'_> {
    pub(crate) fn operation(self) -> Operation {
        match self {
            Self::AgingStart => Operation::AgingStart,
            Self::Begin { .. } => Operation::Begin,
            Self::Reserve(_) => Operation::Reserve,
            Self::Forward { .. } => Operation::Forward,
            Self::Cancel { .. } => Operation::Cancel,
            Self::RollbackStart { .. } => Operation::RollbackStart,
            Self::Clr { .. } => Operation::Clr,
            Self::StatementFinish { .. } => Operation::StatementFinish,
            Self::Commit { .. } => Operation::Commit,
            Self::AbortEnd { .. } => Operation::AbortEnd,
            Self::Prune { .. } => Operation::Prune,
            Self::ReleaseRole { .. } => Operation::ReleaseRole,
            Self::RootFree { .. } => Operation::RootFree,
            Self::TerminalFinish { .. } => Operation::TerminalFinish,
        }
    }
}

/// Borrowed before observations of one original stabilized version. Proof-only
/// pages (`undo` for Cancel/RollbackStart, `previous` for H, the anchor) are
/// read and never become natives. No callback or I/O is reachable from here.
pub(crate) struct Inputs<'a> {
    pub(crate) profile: crate::storage::control::Profile,
    pub(crate) frame: FrameExpectation,
    pub(crate) change: Change<'a>,
    pub(crate) jobs: &'a [[u8; JOB_BYTES]; JOB_COUNT],
    pub(crate) root: &'a [u8; PAGE_BYTES],
    /// The Jobs page holding the changed Job, when one changes.
    pub(crate) jobs_page: Option<&'a [u8; PAGE_BYTES]>,
    pub(crate) status: Option<&'a [u8; PAGE_BYTES]>,
    /// U: the changed Undo page, or the Cancel/RollbackStart proof page.
    pub(crate) undo: Option<&'a [u8; PAGE_BYTES]>,
    /// H: the unchanged nearest retained predecessor (net0 proof).
    pub(crate) previous: Option<&'a [u8; PAGE_BYTES]>,
    pub(crate) table: Option<TableSource<'a>>,
    pub(crate) transaction: Option<&'a [u8; 1536]>,
    pub(crate) aging: Option<&'a [u8; 1536]>,
}

/// Numbers consumed by a Reserve, observed in its own after state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Issued {
    pub(crate) row: RowId,
    pub(crate) undo: UndoRef,
    pub(crate) sequence: u64,
    pub(crate) mutation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Observation {
    pub(crate) operation: Operation,
    pub(crate) header: WalFrameHeader,
    /// Output roles in physical redo order, followed by unused zeros.
    pub(crate) order: [u8; 5],
    pub(crate) count: u8,
    pub(crate) frame_bytes: usize,
    pub(crate) job_ordinal: Option<usize>,
    pub(crate) actor: Option<TxnRef>,
    pub(crate) status: Option<StatusSlotRef>,
    pub(crate) issued: Option<Issued>,
}

/// One changed page, before image borrowed from the inputs.
#[derive(Clone, Copy)]
struct Changed<'a> {
    identity: PageIdentity,
    kind: PageKind,
    before: &'a [u8; PAGE_BYTES],
    before_slot: Option<u16>,
    after_slot: Option<u16>,
}

/// Row edit of the selected RowLeaf, prepared before any output write.
#[derive(Clone, Copy)]
enum LeafEdit<'a> {
    /// Forward: the candidate's sealed cell, already checked against the Job.
    Insert(&'a [u8]),
    Remove(RowId),
    /// Prune: the restamped cell in `Prepared::cell`.
    Replace,
}

/// Every fallible input-dependent value, computed before the first output
/// byte. Writing pages and framing only place these values afterwards. The
/// arrays are disjoint parts of one `ROW_REPLAY_SCRATCH_BYTES` backing.
struct Prepared<'a, 's> {
    op: Operation,
    header: WalFrameHeader,
    frame_bytes: usize,
    row_cell: usize,
    predecessor: bool,
    ordinal: Option<usize>,
    actor: Option<TxnRef>,
    location: Option<StatusSlotRef>,
    issued: Option<Issued>,
    job: &'s mut [u8; JOB_BYTES],
    root: &'s mut [u8; ROOT_BYTES],
    status: Option<StatusHome>,
    undo: &'s mut [u8; UNDO_RECORD_BYTES],
    leaf: Option<(
        LeafEdit<'a>,
        crate::storage::tree_page::TreeExpectation,
        RowSchemaInput<'a>,
    )>,
    cell: &'s mut [u8; MAX_CELL],
    cell_len: usize,
    /// Tx then Aging for XHG; the single owner's descriptor first for XSG.
    descriptors: &'s mut [[u8; 1536]; 2],
    core: &'s mut [u8; 256],
    aux: graph::Aux<'s>,
    aggregate: &'s mut [u8; AGGREGATE_BYTES],
    pages: [Option<Changed<'a>>; 5],
    aging_before: Option<&'a [u8; 1536]>,
    transaction_before: Option<&'a [u8; 1536]>,
}

/// Produce one complete C-owned group into caller loans. Every refusal that
/// depends on the inputs returns before the first `after`/`frame` byte is
/// written. Forward and CLR are A's and return Unsupported here.
#[allow(
    dead_code,
    reason = "owned-backing producer; the live Row install uses produce_borrowed"
)]
pub(crate) fn produce(
    inputs: &Inputs<'_>,
    after: &mut [[u8; PAGE_BYTES]; 5],
    frame: &mut [u8],
) -> Result<Observation> {
    produce_with(inputs, after, frame, &mut [0; ROW_REPLAY_SCRATCH_BYTES])
}

/// `produce` over a caller loan instead of a local backing, for the live Row
/// install whose source peak cannot hold a second backing. The loan's prior
/// bytes have no meaning here: it is cleared first, so the body starts from
/// the same zero backing as `produce`, and on return it holds only
/// discardable prepared values. Output is byte for byte that of `produce`.
#[allow(dead_code, reason = "its live caller is the Row Begin install")]
pub(crate) fn produce_borrowed(
    inputs: &Inputs<'_>,
    after: &mut [[u8; PAGE_BYTES]; 5],
    frame: &mut [u8],
    scratch: &mut [u8; ROW_REPLAY_SCRATCH_BYTES],
) -> Result<Observation> {
    scratch.fill(0);
    produce_with(inputs, after, frame, scratch)
}

fn produce_with(
    inputs: &Inputs<'_>,
    after: &mut [[u8; PAGE_BYTES]; 5],
    frame: &mut [u8],
    backing: &mut [u8; ROW_REPLAY_SCRATCH_BYTES],
) -> Result<Observation> {
    if matches!(inputs.change, Change::Forward { .. } | Change::Clr { .. }) {
        return Err(Error::Unsupported);
    }
    let mut prepared = prepare::new_prepared(inputs, backing);
    prepare::prepare(inputs, None, &mut prepared)?;
    if frame.len() < prepared.frame_bytes {
        return Err(Error::InvalidInput);
    }
    let observation = run(inputs, &mut prepared, after)?;
    let output = &mut frame[..prepared.frame_bytes];
    frame::encode(inputs, &prepared, after, output)?;
    Ok(observation)
}

/// The whole-group observer: recompute the canonical successor from the same
/// stable before observations and compare every byte of the candidate —
/// header, both descriptors, core, natives, Edge/Count/InventoryDelta and
/// redo. Forward's inserted row cell is the one candidate value adopted, and
/// only after its record version, length and projection match the Job.
#[allow(
    dead_code,
    reason = "owned-backing observer; replay uses observe_borrowed"
)]
pub(crate) fn observe<'a>(
    inputs: &Inputs<'a>,
    bytes: &'a [u8],
    after: &mut [[u8; PAGE_BYTES]; 5],
) -> Result<Observation> {
    observe_with(inputs, bytes, after, &mut [0; ROW_REPLAY_SCRATCH_BYTES])
}

/// The Row replay arm's exclusive loan of the WORK_END zero tail. Never hide
/// a nonzero original tail by clearing it on entry; once lent, the whole loan
/// is zero again on every success, error and unwind (Drop).
struct ReplayLoan<'s>(&'s mut [u8; ROW_REPLAY_SCRATCH_BYTES]);
impl<'s> ReplayLoan<'s> {
    fn new(bytes: &'s mut [u8; ROW_REPLAY_SCRATCH_BYTES]) -> Result<Self> {
        if !super::zero(bytes) {
            return Err(Error::InvalidInput);
        }
        Ok(Self(bytes))
    }
}
impl Drop for ReplayLoan<'_> {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

/// `observe` over the caller's zero tail instead of a local backing: the same
/// canonical comparison, byte for byte. The tail is zero again before any
/// success or error reaches the replay owner.
pub(crate) fn observe_borrowed<'a>(
    inputs: &Inputs<'a>,
    bytes: &'a [u8],
    after: &mut [[u8; PAGE_BYTES]; 5],
    scratch: &mut [u8; ROW_REPLAY_SCRATCH_BYTES],
) -> Result<Observation> {
    let loan = ReplayLoan::new(scratch)?;
    observe_with(inputs, bytes, after, &mut *loan.0)
}

fn observe_with<'a>(
    inputs: &Inputs<'a>,
    bytes: &'a [u8],
    after: &mut [[u8; PAGE_BYTES]; 5],
    backing: &mut [u8; ROW_REPLAY_SCRATCH_BYTES],
) -> Result<Observation> {
    let layout = super::decode_layout(bytes, inputs.frame, inputs.profile)?;
    if layout.operation() != inputs.change.operation() {
        return Err(Error::IdentityMismatch);
    }
    let inserted = if layout.operation() == Operation::Forward {
        layout
            .native_records()
            .find(|native| native.kind() == 1)
            .and_then(|native| native.after())
    } else {
        None
    };
    let mut prepared = prepare::new_prepared(inputs, backing);
    prepare::prepare(inputs, inserted, &mut prepared)?;
    if bytes.len() != prepared.frame_bytes {
        return Err(Error::Corrupt);
    }
    let observation = run(inputs, &mut prepared, after)?;
    frame::verify(inputs, &prepared, after, layout)?;
    Ok(observation)
}

/// Place the prepared images, then bind the Aging after to the after Root.
fn run(
    inputs: &Inputs<'_>,
    prepared: &mut Prepared<'_, '_>,
    after: &mut [[u8; PAGE_BYTES]; 5],
) -> Result<Observation> {
    prepare::write_pages(inputs, prepared, after)?;
    prepare::finish_aging(inputs, prepared, after)?;
    let mut order = [0u8; 5];
    let mut count = 0;
    for role in frame::physical_order(prepared)?.into_iter().flatten() {
        order[count] = role as u8;
        count += 1;
    }
    Ok(Observation {
        operation: prepared.op,
        header: prepared.header,
        order,
        count: count as u8,
        frame_bytes: prepared.frame_bytes,
        job_ordinal: prepared.ordinal,
        actor: prepared.actor,
        status: prepared.location,
        issued: prepared.issued,
    })
}

/// Own frame of `produce_with`/`observe_with`: the prepared carrier
/// (references into the backing), the returned observation, the decoded
/// candidate layout and the arguments.
fn entry_bytes() -> usize {
    use std::mem::size_of;
    size_of::<Prepared<'_, '_>>()
        + size_of::<Result<Observation>>()
        + 2 * size_of::<Observation>()
        + size_of::<super::LayoutView<'_>>()
        + size_of::<Result<super::LayoutView<'_>>>()
        + size_of::<Option<&[u8]>>()
        + size_of::<(
            &Inputs<'_>,
            &[u8],
            &mut [[u8; PAGE_BYTES]; 5],
            &mut [u8; ROW_REPLAY_SCRATCH_BYTES],
        )>()
        + 16 * size_of::<usize>()
        + source::slack()
}

/// The entry frame plus the deepest of the sequential phases (backing split,
/// prepare, page/Aging placement, framing/comparison), each with its counted
/// callee chains (`source`). The backing and caller loans (5 pages, frame) are not
/// counted here.
fn body_bytes() -> usize {
    entry_bytes()
        + prepare::split_bytes()
            .max(prepare::source_bytes())
            .max(frame::source_bytes())
            .max(prepare::write_source_bytes())
}

/// Counted nonrecursive source bound of `produce`/`observe`: the local
/// backing array, the wrapper's arguments and result, then the body. Caller
/// loans (5 pages, frame) are charged by the caller.
#[allow(
    dead_code,
    reason = "owned produce/observe bound; the live Row install counts produce_borrowed"
)]
pub(crate) fn source_bytes() -> usize {
    use std::mem::size_of;
    size_of::<[u8; ROW_REPLAY_SCRATCH_BYTES]>()
        + size_of::<(&Inputs<'_>, &[u8], &mut [[u8; PAGE_BYTES]; 5])>()
        + size_of::<Result<Observation>>()
        + body_bytes()
}

/// Counted source bound of `produce_borrowed`. The lent scratch is the
/// caller's and is not counted. The wrapper keeps its arguments and result
/// while the body runs; the clearing fill is a completed call before it.
#[allow(dead_code, reason = "its live caller is the Row Begin install census")]
pub(crate) fn produce_borrowed_source_bytes() -> usize {
    use std::mem::size_of;
    size_of::<(
        &Inputs<'_>,
        &mut [[u8; PAGE_BYTES]; 5],
        &mut [u8],
        &mut [u8; ROW_REPLAY_SCRATCH_BYTES],
    )>() + size_of::<Result<Observation>>()
        + size_of::<(&mut [u8], std::slice::IterMut<'_, u8>, u8)>()
        + body_bytes()
}

/// Counted source bound of `observe_borrowed`. The lent tail is the replay
/// owner's and is not counted. The wrapper keeps its arguments, the loan and
/// the result while the body runs; the entry zero check and the restoring
/// fill (Drop) are completed calls before and after it, charged additively.
pub(crate) fn observe_borrowed_source_bytes() -> usize {
    use std::mem::size_of;
    size_of::<(
        &Inputs<'_>,
        &[u8],
        &mut [[u8; PAGE_BYTES]; 5],
        &mut [u8; ROW_REPLAY_SCRATCH_BYTES],
    )>() + source::returned::<ReplayLoan<'_>, Error>()
        + size_of::<Result<Observation>>()
        + size_of::<(&[u8], std::slice::Iter<'_, u8>, bool)>()
        + body_bytes()
}

/// Counted bound of `ledger::validate_transaction`/`validate_actor`, the
/// per-Terminal check of the scope3 replay closure.
pub(crate) fn ledger_source_bytes() -> usize {
    source::ledger()
}

/// Counted bound of a tree page decode/search/entry parse, the scope3 replay
/// owner's CatalogLeaf schema lookup.
pub(crate) fn tree_source_bytes() -> usize {
    source::tree_codec()
}

#[cfg(test)]
#[path = "transition_tests.rs"]
mod tests;
