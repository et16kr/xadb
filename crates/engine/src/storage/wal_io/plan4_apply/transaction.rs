//! Original-session transaction and statement ownership. Durable actors come
//! only from the original admitted Begin; callers cannot submit a read-set.
use super::*;
use crate::storage::{
    session_fence::SessionFence,
    undo_ref::{UNDO_REF_BYTES, UndoRef},
};
use std::{
    cell::Cell,
    sync::{MutexGuard, atomic::Ordering},
};

#[path = "transaction_catalog.rs"]
pub(super) mod catalog;
#[path = "transaction_write.rs"]
pub(super) mod writable;

const SLOTS: usize = 32;
const HEADER: usize = 64;
const SLOT: usize = 128;
pub(super) const HANDLE_BYTES: usize = 512;
pub(super) const LONG_SOURCE_BYTES: usize = 49_152;
/// Slot actor kind (slot@2, RAM only): 0 read-only, 1 Catalog writer, 2 Row
/// writer. Written once when Begin binds the actor; entries dispatch or
/// refuse by it (XADB-MODULE-ROW-BEGIN-001).
pub(super) const CATALOG_KIND: u8 = 1;
pub(super) const ROW_KIND: u8 = 2;
/// Slot bytes 88..128: the active statement's mark, a canonical 40-byte
/// `Option<UndoRef>` whose None is all zero (T10 slot layout, §6.2).
const MARK: std::ops::Range<usize> = 88..SLOT;
const _: () = assert!(MARK.end - MARK.start == UNDO_REF_BYTES);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum IsolationLevel {
    #[default]
    Serializable,
    ReadCommitted,
}
/// Disk requests; distinct from the volatile substrate's budget options.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TransactionOptions {
    pub(crate) isolation: IsolationLevel,
    pub(crate) read_only: bool,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Active,
    MustAbort,
    Aborted,
    Committed,
    Ended,
}

struct IsolationLease<'v, 'a, 'c, 's> {
    ledger: &'v Mutex<IsolationLedger<'a, 'c, 's>>,
    visibility: &'v BoundVisibility<'a, 'c, 's>,
    fence: &'s SessionFence,
    slot: usize,
    generation: u64,
    actor: Option<TxnRef>,
    /// The bound actor's kind, equal to slot@2 at every check.
    kind: u8,
    active: bool,
}
/// One fixed source charge, returned only to its original live slot/generation.
/// RAM accounting only: no pin, WAL, snapshot or statement authority.
pub(crate) struct CallerCredit<'v, 'a, 'c, 's> {
    ledger: &'v Mutex<IsolationLedger<'a, 'c, 's>>,
    fence: &'s SessionFence,
    slot: usize,
    generation: u64,
    active: bool,
}

pub(crate) struct RowTransaction<'v, 'a, 'c, 's> {
    snapshot: Option<SnapshotGuard<'v, 'a, 'c, 's>>,
    lease: IsolationLease<'v, 'a, 'c, 's>,
    phase: Cell<Phase>,
    next_statement: Cell<u64>,
}
pub(crate) struct RowStatementView<'t, 'v, 'a, 'c, 's> {
    transaction: &'t RowTransaction<'v, 'a, 'c, 's>,
    rc_snapshot: Option<SnapshotGuard<'v, 'a, 'c, 's>>,
    sequence: u64,
    /// The mark written to slot bytes 88..128 at begin: a Row statement's
    /// ATT `undo_head`; None for a Catalog or read-only statement. Every
    /// boundary compares it with the slot ("view mark == slot mark").
    mark: Option<UndoRef>,
    phase: Cell<Phase>,
    revision: Cell<u64>,
    source: CallerCredit<'v, 'a, 'c, 's>,
}

#[allow(
    clippy::large_enum_variant,
    reason = "same borrowed capsule returned inline; charged to its 512-byte source envelope"
)]
pub(crate) enum StatementFinishError<V> {
    Before { error: RetentionError, view: V },
    Fatal(RetentionError),
}
#[allow(
    clippy::large_enum_variant,
    reason = "same borrowed capsule returned inline; charged to its 512-byte source envelope"
)]
pub(crate) enum TransactionFinishError<T> {
    Before {
        error: RetentionError,
        transaction: T,
    },
    RollbackRequired {
        error: RetentionError,
        transaction: T,
    },
    Rejected(RetentionError),
    Fatal(RetentionError),
}
/// Observation of a completed read-only transaction, never a WAL receipt.
pub(crate) struct ReadOnlyCommit {
    q: u64,
}
impl ReadOnlyCommit {
    pub(in crate::storage) fn q(&self) -> u64 {
        self.q
    }
}

fn read64(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(
        bytes[at..at + 8]
            .try_into()
            .expect("bounded isolation field"),
    )
}
fn write64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn read32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(
        bytes[at..at + 4]
            .try_into()
            .expect("bounded isolation field"),
    )
}
fn write32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn row(bytes: &[u8], slot: usize) -> &[u8] {
    &bytes[HEADER + slot * SLOT..HEADER + (slot + 1) * SLOT]
}
fn row_mut(bytes: &mut [u8], slot: usize) -> &mut [u8] {
    &mut bytes[HEADER + slot * SLOT..HEADER + (slot + 1) * SLOT]
}
/// The slot bytes of `mark`: zero for None, the `UndoRef` wire identity for
/// Some. A valid `UndoRef` never encodes to zero (its file id and both
/// generations are nonzero), so the two cases cannot alias.
fn encode_mark(mark: Option<UndoRef>) -> Option<[u8; UNDO_REF_BYTES]> {
    let mut bytes = [0; UNDO_REF_BYTES];
    match mark {
        None => Some(bytes),
        Some(reference) => reference.encode(&mut bytes).ok().map(|()| bytes),
    }
}
/// The mark held by slot bytes, or None when they are not a canonical mark.
/// `UndoRef::decode` fixes every byte it accepts, so equal decoded marks
/// mean equal bytes.
fn decode_mark(bytes: &[u8]) -> Option<Option<UndoRef>> {
    if bytes.len() != UNDO_REF_BYTES {
        return None;
    }
    if bytes.iter().all(|&b| b == 0) {
        return Some(None);
    }
    UndoRef::decode(bytes).ok().map(Some)
}
/// The slot row holds the active statement `sequence` with exactly `mark`.
/// The Catalog empty mark is the None special case of this one comparison.
fn statement_slot(target: &[u8], sequence: u64, mark: Option<UndoRef>) -> bool {
    read64(target, 80) == sequence && decode_mark(&target[MARK]) == Some(mark)
}
fn fail(fence: &SessionFence, cause: RetentionCause) -> RetentionError {
    fence.fence();
    RetentionError {
        phase: RetentionPhase::Census,
        cause,
        effect: Effect::CallerMustResolve,
    }
}
fn vis(error: VisibilityError) -> RetentionError {
    RetentionError {
        phase: RetentionPhase::Census,
        cause: RetentionCause::Visibility(error),
        effect: error.effect,
    }
}
fn entry<'v, 'a, 'c, 's>(
    ledger: &'v Mutex<IsolationLedger<'a, 'c, 's>>,
    fence: &SessionFence,
) -> Result<MutexGuard<'v, IsolationLedger<'a, 'c, 's>>> {
    if std::thread::panicking() {
        fence.fence();
    }
    if fence.is_fenced() {
        return Err(fail(fence, RetentionCause::Fenced));
    }
    let ledger = ledger
        .lock()
        .map_err(|_| fail(fence, RetentionCause::Poisoned))?;
    if fence.is_fenced() {
        return Err(fail(fence, RetentionCause::Fenced));
    }
    if !std::ptr::eq(ledger.fence, fence)
        || !ledger.core.uses_fence(fence)
        || ledger.binding.load(Ordering::Acquire) != super::super::super::BOUND
    {
        return Err(fail(fence, RetentionCause::BindingMismatch));
    }
    Ok(ledger)
}
fn check_slot(bytes: &[u8], slot: usize, generation: u64, fence: &SessionFence) -> Result<()> {
    if slot >= SLOTS
        || generation == 0
        || !(1..=4).contains(&row(bytes, slot)[0])
        || row(bytes, slot)[2] > ROW_KIND
        || read64(row(bytes, slot), 16) != generation
    {
        return Err(fail(fence, RetentionCause::Invariant));
    }
    Ok(())
}

impl<'v, 'a, 'c, 's> IsolationLease<'v, 'a, 'c, 's> {
    fn reserve(
        ledger: &'v Mutex<IsolationLedger<'a, 'c, 's>>,
        visibility: &'v BoundVisibility<'a, 'c, 's>,
        fence: &'s SessionFence,
        isolation: IsolationLevel,
    ) -> Result<Self> {
        let mut state = entry(ledger, fence)?;
        let bytes = &mut state.bytes;
        let slot = (0..SLOTS)
            .find(|&i| row(*bytes, i)[0] == 0)
            .ok_or_else(|| unchanged(RetentionPhase::Census, RetentionCause::Capacity))?;
        let generation = read64(*bytes, 0);
        let next = generation
            .checked_add(1)
            .filter(|_| generation != 0)
            .ok_or_else(|| unchanged(RetentionPhase::Census, RetentionCause::Exhausted))?;
        let used = read64(*bytes, 16)
            .checked_add(HANDLE_BYTES as u64)
            .filter(|&v| v <= LONG_SOURCE_BYTES as u64)
            .ok_or_else(|| unchanged(RetentionPhase::Census, RetentionCause::Capacity))?;
        let count = read64(*bytes, 8)
            .checked_add(1)
            .filter(|&v| v <= SLOTS as u64)
            .ok_or_else(|| fail(fence, RetentionCause::Invariant))?;
        let target = row_mut(*bytes, slot);
        target.fill(0);
        target[0] = 1;
        target[1] = u8::from(isolation == IsolationLevel::ReadCommitted);
        // actor kind/bytes remain zero: no durable actor exists for read-only.
        write32(target, 12, HANDLE_BYTES as u32);
        write64(target, 16, generation);
        write64(*bytes, 0, next);
        write64(*bytes, 8, count);
        write64(*bytes, 16, used);
        Ok(Self {
            ledger,
            visibility,
            fence,
            slot,
            generation,
            actor: None,
            kind: 0,
            active: true,
        })
    }

    fn check(&self, bytes: &[u8]) -> Result<()> {
        if !self.active {
            return Err(unchanged(
                RetentionPhase::Census,
                RetentionCause::WrongState,
            ));
        }
        check_slot(bytes, self.slot, self.generation, self.fence)?;
        let target = row(bytes, self.slot);
        let matches = match self.actor {
            None => {
                self.kind == 0
                    && target[0] == 1
                    && target[2] == 0
                    && target[32..80].iter().all(|&b| b == 0)
            }
            Some(actor) => {
                matches!(self.kind, CATALOG_KIND | ROW_KIND)
                    && target[2] == self.kind
                    && TxnRef::decode(&target[32..80]).ok() == Some(actor)
            }
        };
        if !matches {
            return Err(fail(self.fence, RetentionCause::Invariant));
        }
        Ok(())
    }
    fn close(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }
        let mut state = entry(self.ledger, self.fence)?;
        self.check(state.bytes)?;
        let target = row(state.bytes, self.slot);
        if !(if self.actor.is_some() {
            matches!(target[0], 3 | 4)
        } else {
            target[0] == 1
        }) || read32(target, 12) != HANDLE_BYTES as u32
            || read64(target, 80) != 0
        {
            return Err(fail(self.fence, RetentionCause::Invariant));
        }
        let count = read64(state.bytes, 8)
            .checked_sub(1)
            .ok_or_else(|| fail(self.fence, RetentionCause::Invariant))?;
        let used = read64(state.bytes, 16)
            .checked_sub(HANDLE_BYTES as u64)
            .ok_or_else(|| fail(self.fence, RetentionCause::Invariant))?;
        row_mut(state.bytes, self.slot).fill(0);
        write64(row_mut(state.bytes, self.slot), 16, self.generation);
        write64(state.bytes, 8, count);
        write64(state.bytes, 16, used);
        self.active = false;
        Ok(())
    }
}
impl Drop for IsolationLease<'_, '_, '_, '_> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
impl<'v, 'a, 'c, 's> CallerCredit<'v, 'a, 'c, 's> {
    fn reserve(lease: &IsolationLease<'v, 'a, 'c, 's>) -> Result<Self> {
        let mut state = entry(lease.ledger, lease.fence)?;
        lease.check(state.bytes)?;
        let global = read64(state.bytes, 16)
            .checked_add(HANDLE_BYTES as u64)
            .filter(|&v| v <= LONG_SOURCE_BYTES as u64)
            .ok_or_else(|| unchanged(RetentionPhase::Census, RetentionCause::Capacity))?;
        let local = read32(row(state.bytes, lease.slot), 12)
            .checked_add(HANDLE_BYTES as u32)
            .ok_or_else(|| fail(lease.fence, RetentionCause::Invariant))?;
        write64(state.bytes, 16, global);
        write32(row_mut(state.bytes, lease.slot), 12, local);
        Ok(Self {
            ledger: lease.ledger,
            fence: lease.fence,
            slot: lease.slot,
            generation: lease.generation,
            active: true,
        })
    }
    fn close(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }
        let mut state = entry(self.ledger, self.fence)?;
        check_slot(state.bytes, self.slot, self.generation, self.fence)?;
        let global = read64(state.bytes, 16)
            .checked_sub(HANDLE_BYTES as u64)
            .ok_or_else(|| fail(self.fence, RetentionCause::Invariant))?;
        let local = read32(row(state.bytes, self.slot), 12)
            .checked_sub(HANDLE_BYTES as u32)
            .ok_or_else(|| fail(self.fence, RetentionCause::Invariant))?;
        write64(state.bytes, 16, global);
        write32(row_mut(state.bytes, self.slot), 12, local);
        self.active = false;
        Ok(())
    }
}
impl CallerCredit<'_, '_, '_, '_> {
    /// Explicit cursor close; returns only this credit to its original slot.
    pub(in crate::storage) fn release_checked(mut self) -> Result<()> {
        self.close()
    }
}
impl Drop for CallerCredit<'_, '_, '_, '_> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
impl RowTransaction<'_, '_, '_, '_> {
    /// A Row-scope writable capsule (bound kind 2). Catalog entries refuse it
    /// by this kind before any gate, source or read-set effect.
    pub(in crate::storage) fn row_scope(&self) -> bool {
        self.lease.kind == ROW_KIND
    }
    fn check(&self) -> Result<()> {
        if self.phase.get() != Phase::Active {
            return Err(unchanged(
                RetentionPhase::Census,
                RetentionCause::WrongState,
            ));
        }
        let state = entry(self.lease.ledger, self.lease.fence)?;
        self.lease.check(state.bytes)?;
        if row(state.bytes, self.lease.slot)[0] != 1 {
            return Err(fail(self.lease.fence, RetentionCause::Invariant));
        }
        Ok(())
    }
    fn set_abort_phase(&self, phase: Phase) -> Result<()> {
        let mut state = entry(self.lease.ledger, self.lease.fence)?;
        self.lease.check(state.bytes)?;
        row_mut(state.bytes, self.lease.slot)[0] = match phase {
            Phase::MustAbort => 2,
            Phase::Aborted => 3,
            Phase::Committed => 4,
            _ => return Err(fail(self.lease.fence, RetentionCause::Invariant)),
        };
        self.phase.set(phase);
        Ok(())
    }
    fn close(&mut self) -> Result<()> {
        if self.phase.get() == Phase::Ended {
            return Ok(());
        }
        // An issued actor can be released only after an actual Commit/AbortEnd
        // handoff. Drop does no WAL/I/O and must not lose a healthy obligation.
        if self.lease.actor.is_some()
            && !matches!(self.phase.get(), Phase::Aborted | Phase::Committed)
        {
            return Err(fail(self.lease.fence, RetentionCause::WrongState));
        }
        self.phase.set(Phase::Ended);
        let snapshot = self
            .snapshot
            .take()
            .map_or(Ok(()), |g| g.release_checked().map_err(vis));
        merge(snapshot, self.lease.close())
    }
}
impl Drop for RowTransaction<'_, '_, '_, '_> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
impl RowStatementView<'_, '_, '_, '_, '_> {
    fn check(&self) -> Result<()> {
        self.transaction.check()?;
        if self.phase.get() != Phase::Active {
            return Err(unchanged(
                RetentionPhase::Census,
                RetentionCause::WrongState,
            ));
        }
        let lease = &self.transaction.lease;
        let state = entry(lease.ledger, lease.fence)?;
        lease.check(state.bytes)?;
        if !statement_slot(row(state.bytes, lease.slot), self.sequence, self.mark) {
            return Err(fail(lease.fence, RetentionCause::Invariant));
        }
        Ok(())
    }
    pub(in crate::storage) fn q(&self) -> Result<u64> {
        self.check()?;
        let guard = self
            .rc_snapshot
            .as_ref()
            .or(self.transaction.snapshot.as_ref())
            .ok_or_else(|| fail(self.transaction.lease.fence, RetentionCause::Invariant))?;
        if !guard
            .confirms(self.transaction.lease.visibility)
            .map_err(vis)?
        {
            return Err(fail(
                self.transaction.lease.fence,
                RetentionCause::BindingMismatch,
            ));
        }
        Ok(guard.q())
    }
    pub(in crate::storage) fn own_actor(&self) -> Option<TxnRef> {
        self.transaction.lease.actor
    }
    /// The view's transaction is a Row-scope writable capsule.
    pub(in crate::storage) fn row_scope(&self) -> bool {
        self.transaction.row_scope()
    }
    pub(in crate::storage) fn revision(&self) -> u64 {
        self.revision.get()
    }
    fn close(&mut self) -> Result<()> {
        if self.phase.get() == Phase::Ended {
            return Ok(());
        }
        self.check()?;
        let lease = &self.transaction.lease;
        {
            let mut state = entry(lease.ledger, lease.fence)?;
            lease.check(state.bytes)?;
            // The ended statement closes its sequence and its mark.
            let target = row_mut(state.bytes, lease.slot);
            write64(target, 80, 0);
            target[MARK].fill(0);
        }
        self.phase.set(Phase::Ended);
        let snapshot = self
            .rc_snapshot
            .take()
            .map_or(Ok(()), |g| g.release_checked().map_err(vis));
        merge(snapshot, self.source.close())
    }
}
impl<'v, 'a, 'c, 's> RowStatementView<'_, 'v, 'a, 'c, 's> {
    /// A cursor's 512-byte charge on the view's original slot. Each cursor
    /// holds its own credit; aggregate shortage is Capacity before any effect.
    /// The credit borrows the view, so neither the statement nor the
    /// transaction can finish while it still holds a charge on the slot.
    pub(in crate::storage) fn reserve_cursor_credit<'r>(
        &'r self,
    ) -> Result<CallerCredit<'r, 'a, 'c, 's>> {
        self.check()?;
        CallerCredit::reserve(&self.transaction.lease)
    }
}
// Only the original apply adapter can borrow this guard or register a read.
// Consumers receive a statement-bound call, never the guard or a raw mask.
impl<'t, 'v, 'a, 'c, 's> RowStatementView<'t, 'v, 'a, 'c, 's> {
    pub(super) fn catalog_snapshot(
        &self,
        ledger: &Mutex<IsolationLedger<'a, 'c, 's>>,
        visibility: Option<&BoundVisibility<'a, 'c, 's>>,
    ) -> Result<&SnapshotGuard<'v, 'a, 'c, 's>> {
        let lease = &self.transaction.lease;
        if !std::ptr::eq(ledger, lease.ledger)
            || !visibility.is_some_and(|v| std::ptr::eq(v, lease.visibility))
        {
            return Err(unchanged(
                RetentionPhase::Census,
                RetentionCause::BindingMismatch,
            ));
        }
        if self.phase.get() != Phase::Active || self.transaction.phase.get() != Phase::Active {
            return Err(unchanged(
                RetentionPhase::Census,
                RetentionCause::WrongState,
            ));
        }
        self.rc_snapshot
            .as_ref()
            .or(self.transaction.snapshot.as_ref())
            .ok_or_else(|| fail(lease.fence, RetentionCause::Invariant))
    }

    pub(super) fn check_catalog_read(&self) -> Result<()> {
        self.check()?;
        let lease = &self.transaction.lease;
        let state = entry(lease.ledger, lease.fence)?;
        let mode = row(state.bytes, lease.slot)[1];
        if !matches!(mode, 0 | 1)
            || (mode == 1) != self.rc_snapshot.is_some()
            || (mode == 0) != self.transaction.snapshot.is_some()
        {
            return Err(fail(lease.fence, RetentionCause::Invariant));
        }
        Ok(())
    }

    /// Called only with a copied candidate and the original physical Status by
    /// the disk read operation. A matching actor is never a snapshot fallback.
    pub(super) fn check_own_catalog(
        &self,
        header: crate::storage::catalog_record::CatalogHeader,
        status: crate::storage::transaction_record::TransactionStatus,
    ) -> Result<bool> {
        use crate::storage::transaction_record::{TransactionState, VersionStamp};
        self.check_catalog_read()?;
        let Some(actor) = self.transaction.lease.actor else {
            return Ok(false);
        };
        if header.stamp != VersionStamp::Transaction(actor) {
            return Ok(false);
        }
        if self.sequence != 1
            || self.revision.get() != 1
            || header.logical.transaction() != actor.id()
            || header.logical.mutation_sequence() != 1
            || header.retired
            || header.undo.is_none()
            || status.reference() != actor
            || status.state() != TransactionState::Active
            || status.undo_head() != header.undo
        {
            return Err(fail(
                self.transaction.lease.fence,
                RetentionCause::Invariant,
            ));
        }
        Ok(true)
    }

    /// Register the actual lookup's Found/Missing result, or the admitted
    /// CREATE planner's conservative dependency immediately before its reads.
    /// No I/O, epoch/publication guard or callback runs under this leaf lock.
    pub(super) fn record_catalog_read(&self) -> Result<()> {
        self.check()?;
        let lease = &self.transaction.lease;
        let mut state = entry(lease.ledger, lease.fence)?;
        lease.check(state.bytes)?;
        let target = row_mut(state.bytes, lease.slot);
        if !statement_slot(target, self.sequence, self.mark) {
            return Err(fail(lease.fence, RetentionCause::Invariant));
        }
        target[3] |= 1;
        Ok(())
    }

    /// Register an issued CheckedTable's table read bit (T10 read mask @4)
    /// with the catalog flag of its actual lookup, in one isolation leaf. The
    /// issuer released publication first; no other guard is held here. Bits
    /// are never cleared by statement end, failure or rollback.
    pub(super) fn record_table_read(&self, slot: u8) -> Result<()> {
        self.check()?;
        let lease = &self.transaction.lease;
        let bit = 1u32
            .checked_shl(u32::from(slot))
            .ok_or_else(|| fail(lease.fence, RetentionCause::Invariant))?;
        let mut state = entry(lease.ledger, lease.fence)?;
        lease.check(state.bytes)?;
        let target = row_mut(state.bytes, lease.slot);
        if !statement_slot(target, self.sequence, self.mark) {
            return Err(fail(lease.fence, RetentionCause::Invariant));
        }
        let mask = read32(target, 4) | bit;
        write32(target, 4, mask);
        target[3] |= 1;
        Ok(())
    }
}

impl Drop for RowStatementView<'_, '_, '_, '_, '_> {
    fn drop(&mut self) {
        if self.phase.get() != Phase::Ended && self.transaction.lease.actor.is_some() {
            // Drop cannot inspect the exclusively borrowed durable ledger.
            // A legacy mutation may have happened while this view was live.
            self.transaction.lease.fence.fence();
            return;
        }
        let _ = self.close();
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    MutationPort<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    fn transaction_entry(
        &self,
        owner: &mut Owner<'c, 's>,
    ) -> Result<&'b BoundVisibility<'a, 'c, 's>> {
        health(self.storage, self.pool)?;
        self.pool
            .check_retention_reader(owner)
            .map_err(|e| frame(RetentionPhase::Census, e))?;
        let visibility = self
            .visibility
            .ok_or_else(|| unchanged(RetentionPhase::Census, RetentionCause::WrongState))?;
        let state = entry(self.isolation, self.storage.session_fence())?;
        if !std::ptr::eq(state.core, self.pool.bound_core())
            || !std::ptr::eq(state.binding, &self.storage.binding)
            || !visibility.matches_core(state.core)
        {
            return Err(fail(state.fence, RetentionCause::BindingMismatch));
        }
        drop(state);
        visibility.isolation_frontier().map_err(vis)?;
        Ok(visibility)
    }
    fn transaction_matches(&self, transaction: &RowTransaction<'_, 'a, 'c, 's>) -> Result<()> {
        if !std::ptr::eq(self.isolation, transaction.lease.ledger)
            || !self
                .visibility
                .is_some_and(|v| std::ptr::eq(v, transaction.lease.visibility))
        {
            return Err(unchanged(
                RetentionPhase::Census,
                RetentionCause::BindingMismatch,
            ));
        }
        transaction.check()
    }
    /// Read-only enters the original slot/credit/snapshot state. Writable Begin
    /// is supplied separately by begin_catalog_transaction with explicit original
    /// Status and flusher loans; automatic selection remains unimplemented.
    pub(in crate::storage) fn begin_transaction(
        &mut self,
        owner: &mut Owner<'c, 's>,
        options: TransactionOptions,
        _private: &mut [u8; 425_984],
    ) -> Result<RowTransaction<'b, 'a, 'c, 's>> {
        let visibility = self.transaction_entry(owner)?;
        if !options.read_only {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::Unsupported,
            ));
        }
        self.reserve_transaction(owner, options.isolation, visibility)
    }

    fn reserve_transaction(
        &mut self,
        owner: &mut Owner<'c, 's>,
        isolation: IsolationLevel,
        visibility: &'b BoundVisibility<'a, 'c, 's>,
    ) -> Result<RowTransaction<'b, 'a, 'c, 's>> {
        let lease = IsolationLease::reserve(
            self.isolation,
            visibility,
            self.storage.session_fence(),
            isolation,
        )?;
        let mut transaction = RowTransaction {
            snapshot: None,
            lease,
            phase: Cell::new(Phase::Active),
            next_statement: Cell::new(1),
        };
        let start = (|| {
            if isolation == IsolationLevel::Serializable {
                let reserved = visibility.reserve_snapshot().map_err(vis)?;
                transaction.snapshot =
                    Some(visibility.register_current(owner, reserved).map_err(vis)?);
            }
            let q = match &transaction.snapshot {
                Some(g) => g.q(),
                None => visibility.isolation_frontier().map_err(vis)?,
            };
            let mut state = entry(self.isolation, transaction.lease.fence)?;
            transaction.lease.check(state.bytes)?;
            write64(row_mut(state.bytes, transaction.lease.slot), 24, q);
            Ok(())
        })();
        if let Err(error) = start {
            return merge(Err(error), transaction.close());
        }
        Ok(transaction)
    }
    pub(in crate::storage) fn begin_statement<'t, 'v>(
        &mut self,
        owner: &mut Owner<'c, 's>,
        transaction: &'t RowTransaction<'v, 'a, 'c, 's>,
    ) -> Result<RowStatementView<'t, 'v, 'a, 'c, 's>> {
        self.transaction_entry(owner)?;
        self.transaction_matches(transaction)?;
        // The statement mark, chosen by the capsule kind written at Begin.
        let mark = match transaction.lease.actor {
            // Row (ROW-BEGIN-001 "statement mark"): an Active actor with no
            // RowReserved root; the mark is its ATT descriptor's undo_head.
            Some(actor) if transaction.row_scope() => {
                self.row_statement_head(actor, RetentionPhase::Prepare)?
            }
            // The first Catalog writable supply has no mutation/rollback
            // statement adapter. Require actual empty Active state, whose
            // mark is the empty None, not a fabricated mark.
            Some(actor) => {
                if !self.empty_actor_active(actor, transaction.lease.kind)? {
                    return Err(unchanged(
                        RetentionPhase::Prepare,
                        RetentionCause::WrongState,
                    ));
                }
                None
            }
            None => None,
        };
        let encoded = encode_mark(mark)
            .ok_or_else(|| fail(transaction.lease.fence, RetentionCause::Invariant))?;
        let sequence = transaction.next_statement.get();
        let next = sequence
            .checked_add(1)
            .ok_or_else(|| unchanged(RetentionPhase::Census, RetentionCause::Exhausted))?;
        let mode = {
            let state = entry(self.isolation, transaction.lease.fence)?;
            let target = row(state.bytes, transaction.lease.slot);
            if read64(target, 80) != 0 {
                return Err(unchanged(RetentionPhase::Census, RetentionCause::Busy));
            }
            // An ended statement closed its mark; a live idle mark is corrupt.
            if !statement_slot(target, 0, None) {
                return Err(fail(transaction.lease.fence, RetentionCause::Invariant));
            }
            target[1]
        };
        let mut source = CallerCredit::reserve(&transaction.lease)?;
        let snapshot = if mode == 1 {
            let visibility = transaction.lease.visibility;
            let registered = visibility
                .reserve_snapshot()
                .map_err(vis)
                .and_then(|r| visibility.register_current(owner, r).map_err(vis));
            match registered {
                Ok(g) => Some(g),
                Err(e) => return merge(Err(e), source.close()),
            }
        } else {
            None
        };
        {
            let mut state = entry(self.isolation, transaction.lease.fence)?;
            transaction.lease.check(state.bytes)?;
            // The mark is RAM slot state only: no WAL (ROW-BEGIN-001).
            let target = row_mut(state.bytes, transaction.lease.slot);
            write64(target, 80, sequence);
            target[MARK].copy_from_slice(&encoded);
        }
        transaction.next_statement.set(next);
        Ok(RowStatementView {
            transaction,
            rc_snapshot: snapshot,
            sequence,
            mark,
            phase: Cell::new(Phase::Active),
            revision: Cell::new(0),
            source,
        })
    }
    #[allow(
        clippy::result_large_err,
        reason = "unchanged failure returns the same inline statement view; its return carrier is charged to the 512-byte source envelope"
    )]
    pub(in crate::storage) fn finish_statement<'t, 'v>(
        &mut self,
        owner: &mut Owner<'c, 's>,
        mut view: RowStatementView<'t, 'v, 'a, 'c, 's>,
    ) -> std::result::Result<(), StatementFinishError<RowStatementView<'t, 'v, 'a, 'c, 's>>> {
        if let Some(actor) = view.transaction.lease.actor {
            // A writable statement ends under the fixed progress class of its
            // capsule kind (ROW-BEGIN-001: finish_statement dispatches by
            // kind; RowProgress reserves the same CatalogProgressCharge).
            let row_scope = view.row_scope();
            let class = if row_scope {
                source::OperationClass::RowProgress
            } else {
                source::OperationClass::CatalogProgress
            };
            let checked = self.transaction_check(owner, class, |writer, owner| {
                writer.transaction_entry(owner)?;
                writer.transaction_matches(view.transaction)?;
                view.check()?;
                if row_scope {
                    // A Reserved or Aborting actor is Before(WrongState). A
                    // clean end writes no WAL and keeps the unused 6080-byte
                    // statement credit for the next mark (consumer contract,
                    // row-shape): nothing durable changes here.
                    writer.row_statement_head(actor, RetentionPhase::Finalize)?;
                } else {
                    writer.check_catalog_statement_finish(&view)?;
                }
                view.close()
            });
            return match checked {
                Ok(()) => Ok(()),
                Err(error) if error.effect == Effect::Unchanged => {
                    Err(StatementFinishError::Before { error, view })
                }
                Err(error) => Err(StatementFinishError::Fatal(error)),
            };
        }
        if let Err(error) = self
            .transaction_entry(owner)
            .and_then(|_| self.transaction_matches(view.transaction))
            .and_then(|_| view.check())
        {
            return if error.effect == Effect::Unchanged {
                Err(StatementFinishError::Before { error, view })
            } else {
                Err(StatementFinishError::Fatal(error))
            };
        }
        view.close().map_err(StatementFinishError::Fatal)
    }
    #[allow(
        clippy::result_large_err,
        reason = "unchanged failure returns the same inline transaction; its return carrier is charged to the 512-byte source envelope"
    )]
    pub(in crate::storage) fn finish_read_only<'v>(
        &mut self,
        owner: &mut Owner<'c, 's>,
        mut transaction: RowTransaction<'v, 'a, 'c, 's>,
    ) -> std::result::Result<ReadOnlyCommit, TransactionFinishError<RowTransaction<'v, 'a, 'c, 's>>>
    {
        let checked = (|| {
            self.transaction_entry(owner)?;
            self.transaction_matches(&transaction)?;
            if transaction.lease.actor.is_some() {
                return Err(unchanged(
                    RetentionPhase::Finalize,
                    RetentionCause::WrongState,
                ));
            }
            let (mode, q, reads, writes, catalog) = {
                let state = entry(self.isolation, transaction.lease.fence)?;
                let target = row(state.bytes, transaction.lease.slot);
                if read64(target, 80) != 0 {
                    return Err(unchanged(RetentionPhase::Finalize, RetentionCause::Busy));
                }
                if target[2] != 0 || target[32..80].iter().any(|&b| b != 0) {
                    return Err(fail(transaction.lease.fence, RetentionCause::Invariant));
                }
                (
                    target[1],
                    read64(target, 24),
                    read32(target, 4),
                    read32(target, 8),
                    target[3],
                )
            };
            if mode == 0
                && !transaction
                    .lease
                    .visibility
                    .validate_isolation(q, reads | writes, catalog != 0)
                    .map_err(vis)?
            {
                return Ok(None);
            }
            Ok(Some(
                transaction
                    .lease
                    .visibility
                    .isolation_frontier()
                    .map_err(vis)?,
            ))
        })();
        match checked {
            Err(error) if error.effect == Effect::Unchanged => {
                Err(TransactionFinishError::Before { error, transaction })
            }
            Err(error) => Err(TransactionFinishError::Fatal(error)),
            Ok(q) => {
                transaction.close().map_err(TransactionFinishError::Fatal)?;
                q.map(|q| ReadOnlyCommit { q }).ok_or_else(|| {
                    TransactionFinishError::Rejected(unchanged(
                        RetentionPhase::Finalize,
                        RetentionCause::SerializationConflict,
                    ))
                })
            }
        }
    }
    /// Consumes the capsule only after the actual AbortEnd and terminal close
    /// of a Row rollback. That adapter is not supplied: Before, no effect.
    #[allow(
        clippy::result_large_err,
        reason = "unchanged failure returns the same inline transaction; its return carrier is charged to the 512-byte source envelope"
    )]
    pub(in crate::storage) fn finish_aborted<'v>(
        &mut self,
        _owner: &mut Owner<'c, 's>,
        transaction: RowTransaction<'v, 'a, 'c, 's>,
    ) -> std::result::Result<(), TransactionFinishError<RowTransaction<'v, 'a, 'c, 's>>> {
        Err(TransactionFinishError::Before {
            error: unchanged(RetentionPhase::Finalize, RetentionCause::Unsupported),
            transaction,
        })
    }
    /// Row Commit keeps the durable ledger borrow from validation to Q
    /// publication. Not supplied yet: Before with the same capsule, no effect.
    #[allow(
        clippy::result_large_err,
        reason = "unchanged failure returns the same inline transaction; its return carrier is charged to the 512-byte source envelope"
    )]
    pub(in crate::storage) fn commit_row<'v>(
        &mut self,
        _owner: &mut Owner<'c, 's>,
        transaction: RowTransaction<'v, 'a, 'c, 's>,
        _private: &mut [u8; 425_984],
    ) -> std::result::Result<
        super::row::RowCommitObservation,
        TransactionFinishError<RowTransaction<'v, 'a, 'c, 's>>,
    > {
        Err(TransactionFinishError::Before {
            error: unchanged(RetentionPhase::Finalize, RetentionCause::Unsupported),
            transaction,
        })
    }
}

/// Caller-owned handle plus its largest creation/return carrier is charged once
/// to the runtime 512-byte lease. Short nested helpers remain in the source peak.
pub(super) fn handle_census() -> (usize, usize) {
    use std::mem::size_of;
    type T<'v, 'a, 'c, 's> = RowTransaction<'v, 'a, 'c, 's>;
    type V<'t, 'v, 'a, 'c, 's> = RowStatementView<'t, 'v, 'a, 'c, 's>;
    (
        size_of::<T<'_, '_, '_, '_>>()
            + size_of::<Result<T<'_, '_, '_, '_>>>()
                .max(size_of::<
                    std::result::Result<ReadOnlyCommit, TransactionFinishError<T<'_, '_, '_, '_>>>,
                >())
                .max(size_of::<
                    std::result::Result<
                        catalog::CatalogCommit,
                        TransactionFinishError<T<'_, '_, '_, '_>>,
                    >,
                >())
                .max(size_of::<
                    std::result::Result<
                        super::row::RowCommitObservation,
                        TransactionFinishError<T<'_, '_, '_, '_>>,
                    >,
                >())
                .max(size_of::<
                    std::result::Result<(), TransactionFinishError<T<'_, '_, '_, '_>>>,
                >()),
        size_of::<V<'_, '_, '_, '_, '_>>()
            + size_of::<Result<V<'_, '_, '_, '_, '_>>>().max(size_of::<
                std::result::Result<(), StatementFinishError<V<'_, '_, '_, '_, '_>>>,
            >()),
    )
}
pub(super) fn source_bytes() -> usize {
    use std::mem::size_of;
    2 * size_of::<MutexGuard<'_, IsolationLedger<'_, '_, '_>>>()
        + 2 * size_of::<Result<MutexGuard<'_, IsolationLedger<'_, '_, '_>>>>()
        + 8 * size_of::<Result<()>>()
        + 4 * size_of::<RetentionError>()
        + 2 * size_of::<IsolationLease<'_, '_, '_, '_>>()
        + 2 * size_of::<CallerCredit<'_, '_, '_, '_>>()
        + 16 * size_of::<u64>()
        + 16 * size_of::<&()>()
        + 2 * size_of::<Result<TxnRef>>()
        + 2 * size_of::<TxnRef>()
        // The statement mark: the view's copy and begin's chosen value, the
        // slot's decoded mark at each boundary, and begin's encoded bytes.
        + 2 * size_of::<Option<UndoRef>>()
        + 2 * size_of::<Option<Option<UndoRef>>>()
        + size_of::<Option<[u8; UNDO_REF_BYTES]>>()
        + size_of::<[u64; 4]>()
        + visibility::source_census().peak_leaf
}

/// Original view validation plus the actual decoded own-CREATE observations.
/// These carriers survive the leaf slot checks; the physical guard is gone.
pub(super) fn catalog_read_source_bytes() -> usize {
    use crate::storage::{catalog_record::CatalogHeader, transaction_record::TransactionStatus};
    use std::mem::size_of;
    source_bytes()
        + 2 * size_of::<CatalogHeader>()
        + 2 * size_of::<TransactionStatus>()
        // status's local Result and merge return remain in lexical scope after
        // checked physical release, while this own-check leaf may acquire a slot.
        + 2 * size_of::<Result<TransactionStatus>>()
        + 2 * size_of::<Result<bool>>()
        + 2 * size_of::<Result<&SnapshotGuard<'_, '_, '_, '_>>>()
        + 8 * size_of::<&()>()
        + size_of::<Option<TxnRef>>()
        + 4 * size_of::<bool>()
}

/// (isolation leaf held, publication leaf held) at the instant of the call.
/// Each lock is only tried and dropped at once; nothing is read or changed.
#[cfg(test)]
pub(in crate::storage) struct LeafProbe<'v, 'a, 'c, 's> {
    ledger: &'v Mutex<IsolationLedger<'a, 'c, 's>>,
    visibility: &'v BoundVisibility<'a, 'c, 's>,
}
#[cfg(test)]
impl<'v, 'a, 'c, 's> RowStatementView<'_, 'v, 'a, 'c, 's> {
    /// Cross-thread probe of this statement's own isolation and publication
    /// leaves, for exact-cut tests. It carries no bytes, mask or authority.
    pub(in crate::storage) fn test_leaf_probe(&self) -> LeafProbe<'v, 'a, 'c, 's> {
        LeafProbe {
            ledger: self.transaction.lease.ledger,
            visibility: self.transaction.lease.visibility,
        }
    }
}
#[cfg(test)]
impl LeafProbe<'_, '_, '_, '_> {
    pub(in crate::storage) fn busy(&self) -> (bool, bool) {
        (
            matches!(
                self.ledger.try_lock(),
                Err(std::sync::TryLockError::WouldBlock)
            ),
            self.visibility.test_publication_busy(),
        )
    }
}

#[cfg(test)]
impl RowStatementView<'_, '_, '_, '_, '_> {
    /// S2W-R01: an ended (stale) view while `stale`, the same Active view
    /// again after; only the view's own phase cell changes, never the slot.
    pub(in crate::storage) fn test_set_stale(&self, stale: bool) {
        self.phase
            .set(if stale { Phase::Ended } else { Phase::Active });
    }

    /// S2W-R18: the next Row statement head check on this thread sees the
    /// actor's Row Job as RootReserved (`writable::test_root_reserved_once`).
    pub(in crate::storage) fn test_root_reserved_once(&self) {
        writable::test_root_reserved_once();
    }

    /// S2W-R03/R16: clears the transaction's catalog flag and table read mask
    /// so that a later registration is observable; write mask untouched.
    pub(in crate::storage) fn test_clear_reads(&self) {
        let lease = &self.transaction.lease;
        let mut state = lease
            .ledger
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let target = row_mut(state.bytes, lease.slot);
        target[3] = 0;
        write32(target, 4, 0);
    }

    pub(in crate::storage) fn test_catalog_reads(&self) -> u8 {
        let lease = &self.transaction.lease;
        let state = lease
            .ledger
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        row(state.bytes, lease.slot)[3]
    }

    /// Scalar (table read mask, table write mask); it cannot set or clear one.
    pub(in crate::storage) fn test_table_masks(&self) -> (u32, u32) {
        let lease = &self.transaction.lease;
        let state = lease
            .ledger
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let target = row(state.bytes, lease.slot);
        (read32(target, 4), read32(target, 8))
    }

    pub(in crate::storage) fn test_sequence(&self) -> u64 {
        self.sequence
    }

    /// The view's own mark; it cannot change the view or the slot.
    pub(in crate::storage) fn test_mark(&self) -> Option<UndoRef> {
        self.mark
    }

    /// Scalar (global used, own slot charge); cannot reserve or return one.
    pub(in crate::storage) fn test_charges(&self) -> (u64, u32) {
        let lease = &self.transaction.lease;
        let state = lease
            .ledger
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        (
            read64(state.bytes, 16),
            read32(row(state.bytes, lease.slot), 12),
        )
    }
}

#[cfg(test)]
impl RowTransaction<'_, '_, '_, '_> {
    /// Scalar (active statement sequence, slot mark bytes 88..128).
    pub(in crate::storage) fn test_statement_slot(&self) -> (u64, [u8; UNDO_REF_BYTES]) {
        let state = self.lease.ledger.lock().unwrap();
        let target = row(state.bytes, self.lease.slot);
        (
            read64(target, 80),
            target[MARK].try_into().expect("40-byte mark"),
        )
    }
    /// Stale-mark fault fixture: overwrite only the slot's mark bytes.
    pub(in crate::storage) fn test_write_statement_mark(&self, bytes: [u8; UNDO_REF_BYTES]) {
        let mut state = self.lease.ledger.lock().unwrap();
        row_mut(state.bytes, self.lease.slot)[MARK].copy_from_slice(&bytes);
    }
}

#[cfg(test)]
#[path = "transaction_mark_tests.rs"]
mod mark_tests;

#[cfg(test)]
impl<W: WriterNamespace> ReadPort<'_, '_, '_, '_, '_, '_, '_, '_, W> {
    /// Synthetic long-class occupancy (MBC10-V07): add `bytes` to the
    /// original long total while `run` executes, then remove exactly them.
    /// No slot, credit or snapshot is issued.
    pub(in crate::storage) fn catalog_test_with_held_long<T>(
        &self,
        bytes: u64,
        run: impl FnOnce() -> T,
    ) -> T {
        let adjust = |add: bool| {
            let mut state = self.isolation.lock().unwrap();
            let used = read64(state.bytes, 16);
            let next = if add { used + bytes } else { used - bytes };
            write64(state.bytes, 16, next);
        };
        adjust(true);
        let result = run();
        adjust(false);
        result
    }
}
