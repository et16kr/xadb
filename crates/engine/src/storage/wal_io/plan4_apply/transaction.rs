//! Original-session read-only transaction and statement ownership. No durable
//! actor is fabricated. Writable Begin and source-dependent reads are supplied
//! by their later adapters; this module never accepts a caller read-set.
use super::*;
use crate::storage::session_fence::SessionFence;
use std::{
    cell::Cell,
    sync::{MutexGuard, atomic::Ordering},
};

const SLOTS: usize = 32;
const HEADER: usize = 64;
const SLOT: usize = 128;
pub(super) const HANDLE_BYTES: usize = 512;
pub(super) const LONG_SOURCE_BYTES: usize = 49_152;

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
    Ended,
}

struct IsolationLease<'v, 'a, 'c, 's> {
    ledger: &'v Mutex<IsolationLedger<'a, 'c, 's>>,
    visibility: &'v BoundVisibility<'a, 'c, 's>,
    fence: &'s SessionFence,
    slot: usize,
    generation: u64,
    active: bool,
}
/// One fixed source charge, returned only to its original live slot/generation.
struct CallerCredit<'v, 'a, 'c, 's> {
    ledger: &'v Mutex<IsolationLedger<'a, 'c, 's>>,
    fence: &'s SessionFence,
    slot: usize,
    generation: u64,
    active: bool,
}

pub(crate) struct RowTransaction<'v, 'a, 'c, 's> {
    snapshot: Option<SnapshotGuard<'v, 'a, 'c, 's>>,
    lease: IsolationLease<'v, 'a, 'c, 's>,
    source: CallerCredit<'v, 'a, 'c, 's>,
    phase: Cell<Phase>,
    next_statement: Cell<u64>,
}
pub(crate) struct RowStatementView<'t, 'v, 'a, 'c, 's> {
    transaction: &'t RowTransaction<'v, 'a, 'c, 's>,
    rc_snapshot: Option<SnapshotGuard<'v, 'a, 'c, 's>>,
    sequence: u64,
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
        || row(bytes, slot)[0] != 1
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
    ) -> Result<(Self, CallerCredit<'v, 'a, 'c, 's>)> {
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
        Ok((
            Self {
                ledger,
                visibility,
                fence,
                slot,
                generation,
                active: true,
            },
            CallerCredit {
                ledger,
                fence,
                slot,
                generation,
                active: true,
            },
        ))
    }
    fn check(&self, bytes: &[u8]) -> Result<()> {
        if !self.active {
            return Err(unchanged(
                RetentionPhase::Census,
                RetentionCause::WrongState,
            ));
        }
        check_slot(bytes, self.slot, self.generation, self.fence)
    }
    fn close(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }
        let mut state = entry(self.ledger, self.fence)?;
        self.check(state.bytes)?;
        let target = row(state.bytes, self.slot);
        if read32(target, 12) != 0 || read64(target, 80) != 0 {
            return Err(fail(self.fence, RetentionCause::Invariant));
        }
        let count = read64(state.bytes, 8)
            .checked_sub(1)
            .ok_or_else(|| fail(self.fence, RetentionCause::Invariant))?;
        row_mut(state.bytes, self.slot).fill(0);
        write64(row_mut(state.bytes, self.slot), 16, self.generation);
        write64(state.bytes, 8, count);
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
impl Drop for CallerCredit<'_, '_, '_, '_> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
impl RowTransaction<'_, '_, '_, '_> {
    fn check(&self) -> Result<()> {
        if self.phase.get() != Phase::Active {
            return Err(unchanged(
                RetentionPhase::Census,
                RetentionCause::WrongState,
            ));
        }
        let state = entry(self.lease.ledger, self.lease.fence)?;
        self.lease.check(state.bytes)
    }
    fn close(&mut self) -> Result<()> {
        if self.phase.get() == Phase::Ended {
            return Ok(());
        }
        self.phase.set(Phase::Ended);
        let snapshot = self
            .snapshot
            .take()
            .map_or(Ok(()), |g| g.release_checked().map_err(vis));
        let credit = self.source.close();
        let lease = self.lease.close();
        merge(merge(snapshot, credit), lease)
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
        let target = row(state.bytes, lease.slot);
        if read64(target, 80) != self.sequence || target[88..].iter().any(|&b| b != 0) {
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
        None
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
            write64(row_mut(state.bytes, lease.slot), 80, 0);
        }
        self.phase.set(Phase::Ended);
        let snapshot = self
            .rc_snapshot
            .take()
            .map_or(Ok(()), |g| g.release_checked().map_err(vis));
        merge(snapshot, self.source.close())
    }
}
impl Drop for RowStatementView<'_, '_, '_, '_, '_> {
    fn drop(&mut self) {
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
    /// remains Unsupported until its actual durable actor adapter is connected.
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
        let (lease, source) = IsolationLease::reserve(
            self.isolation,
            visibility,
            self.storage.session_fence(),
            options.isolation,
        )?;
        let mut transaction = RowTransaction {
            snapshot: None,
            lease,
            source,
            phase: Cell::new(Phase::Active),
            next_statement: Cell::new(1),
        };
        let start = (|| {
            if options.isolation == IsolationLevel::Serializable {
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
            write64(row_mut(state.bytes, transaction.lease.slot), 80, sequence);
        }
        transaction.next_statement.set(next);
        Ok(RowStatementView {
            transaction,
            rc_snapshot: snapshot,
            sequence,
            phase: Cell::new(Phase::Active),
            revision: Cell::new(0),
            source,
        })
    }
    pub(in crate::storage) fn finish_statement<'t, 'v>(
        &mut self,
        owner: &mut Owner<'c, 's>,
        mut view: RowStatementView<'t, 'v, 'a, 'c, 's>,
    ) -> std::result::Result<(), StatementFinishError<RowStatementView<'t, 'v, 'a, 'c, 's>>> {
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
    pub(in crate::storage) fn finish_read_only<'v>(
        &mut self,
        owner: &mut Owner<'c, 's>,
        mut transaction: RowTransaction<'v, 'a, 'c, 's>,
    ) -> std::result::Result<ReadOnlyCommit, TransactionFinishError<RowTransaction<'v, 'a, 'c, 's>>>
    {
        let checked = (|| {
            self.transaction_entry(owner)?;
            self.transaction_matches(&transaction)?;
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
}

/// Caller-owned handle plus its largest creation/return carrier is charged once
/// to the runtime 512-byte lease. Short nested helpers remain in the source peak.
pub(super) fn handle_census() -> (usize, usize) {
    use std::mem::size_of;
    type T<'v, 'a, 'c, 's> = RowTransaction<'v, 'a, 'c, 's>;
    type V<'t, 'v, 'a, 'c, 's> = RowStatementView<'t, 'v, 'a, 'c, 's>;
    (
        size_of::<T<'_, '_, '_, '_>>()
            + size_of::<Result<T<'_, '_, '_, '_>>>().max(size_of::<
                std::result::Result<ReadOnlyCommit, TransactionFinishError<T<'_, '_, '_, '_>>>,
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
        + visibility::source_census().peak_leaf
}

#[cfg(test)]
impl RowStatementView<'_, '_, '_, '_, '_> {
    pub(in crate::storage) fn test_sequence(&self) -> u64 {
        self.sequence
    }
}
