//! Serialized empty lifecycle using the original pool and WAL authority.
//! Stable directory census ends before producer/installation temporaries begin.
use super::{Cause, EmptyTransactionError, EmptyTransactions, Result, frame, unchanged};
use crate::{
    storage::{
        allocation_page::{self, AllocationEntry, ReservationOwner},
        control::{ControlKind, ControlRef, PROFILE_BYTES, Profile},
        dirty_frame::{self, FrameStamp, TargetRequest},
        obligation::{DescriptorSpec, OwnerState, RemainingBudget, decode_descriptor},
        page::{PAGE_BYTES, PageIdentity, PageKind, PageLsn, PageRef},
        page_io::{PageFileReader, PageIoError},
        semantic_group::{self, RollbackStartExpected, begin, empty_abort},
        status_home::{StatusHome, decode_status_home_page},
        transaction_record::{
            RollbackState, StatusSlotRef, TransactionState, TransactionStatusCell, TxnRef,
        },
        wal_frame::{FrameExpectation, WalFrameHeader},
        wal_io::{DirtyInstallPermit, MutableStorageOwner, WalPrefix, WalRequirement},
    },
    transaction::{
        TxnId,
        concurrency::{Owner, PageConcurrency},
    },
};
use foundation::{
    codec::content_digest,
    error::{Certainty, Effect},
    identity::Generation,
    namespace::{NamespaceErrorKind, WriterNamespace},
};
use std::mem::size_of;

const SLOT: usize = 1664;
const LOCATION: std::ops::Range<usize> = 1536..1576;
const ACTOR: std::ops::Range<usize> = 1576..1624;
const PHASE: usize = 1624;
const STABLE: u8 = 1;
const STAGED: u8 = 2;
const PAGE_INSTALLED: u8 = 3;
const DESCRIPTOR_INSTALLED: u8 = 4;
const DPT_INSTALLED: u8 = 5;
const FRAME_BYTES: usize = 3040;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EmptyTransactionObservation {
    pub(crate) actor: TxnRef,
    pub(crate) location: StatusSlotRef,
    pub(crate) state: TransactionState,
    pub(crate) lsn: PageLsn,
    pub(crate) issued_local: u64,
    pub(crate) adopted_local: u64,
    pub(crate) retained_transactions: usize,
    pub(crate) remaining_wal: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Change {
    Begin,
    Rollback,
    Abort,
}
#[derive(Clone, Copy)]
struct Ledger {
    occupied: u32,
    free: Option<usize>,
    steps: u64,
    wal: u64,
}
struct Selection {
    request: TargetRequest,
    stamp: FrameStamp,
    home: StatusHome,
}

/// No raw constructor or Clone. Only `change` seals a producer image after the
/// actual stage is retained by the original owner. The leaf adapter consumes it.
pub(in crate::storage) struct EmptyTransactionInstall<'a, 'c, 's> {
    core: &'c PageConcurrency<'s>,
    stamp: FrameStamp,
    requirement: WalRequirement<'s>,
    after: &'a [u8; PAGE_BYTES],
}
impl<'a, 'c, 's> EmptyTransactionInstall<'a, 'c, 's> {
    pub(in crate::storage) fn core(&self) -> &'c PageConcurrency<'s> {
        self.core
    }
    pub(in crate::storage) fn stamp(&self) -> FrameStamp {
        self.stamp
    }
    pub(in crate::storage) fn requirement(&self) -> WalRequirement<'s> {
        self.requirement
    }
    pub(in crate::storage) fn after(&self) -> &'a [u8; PAGE_BYTES] {
        self.after
    }
}
fn fatal<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    cause: Cause,
) -> EmptyTransactionError {
    storage.session_fence().fence();
    EmptyTransactionError {
        cause,
        effect: Effect::CallerMustResolve,
    }
}
fn merge<T>(result: Result<T>, cleanup: Result<()>) -> Result<T> {
    match (result, cleanup) {
        (Err(original), _) if original.effect == Effect::CallerMustResolve => Err(original),
        (_, Err(cleanup)) => Err(cleanup),
        (result, Ok(())) => result,
    }
}
fn profile_ref(profile: Profile) -> ControlRef {
    ControlRef {
        kind: ControlKind::Profile,
        slot: 0,
        index: 0,
        generation: profile.spec().generation,
        encoded_len: PROFILE_BYTES as u32,
        digest: profile.digest(),
    }
}
fn row(bank: &[u8], index: usize) -> &[u8] {
    &bank[index * SLOT..(index + 1) * SLOT]
}
fn row_mut(bank: &mut [u8], index: usize) -> &mut [u8] {
    &mut bank[index * SLOT..(index + 1) * SLOT]
}

/// The bank is a live owner ledger, not merely a valid descriptor codec. Its
/// empty-only shape must hold before the broader RollbackStart producer runs.
fn descriptor(
    bank_row: &[u8],
    profile: Profile,
    prefix: WalPrefix,
    allow_terminal: bool,
) -> Option<(TxnRef, StatusSlotRef, DescriptorSpec, u64)> {
    let actor = TxnRef::decode(&bank_row[ACTOR]).ok()?;
    let location = StatusSlotRef::decode(&bank_row[LOCATION]).ok()?;
    if location.slot() != 0
        || location.generation() != actor.generation()
        || actor.id().database != profile.spec().namespace.database
        || actor.id().timeline != profile.spec().namespace.timeline
        || actor.id().writer_epoch != prefix.stream.writer_epoch
        || bank_row[PHASE + 1..].iter().any(|byte| *byte != 0)
    {
        return None;
    }
    let d = decode_descriptor(
        &bank_row[..1536],
        ReservationOwner::Transaction(actor),
        profile_ref(profile),
    )
    .ok()?;
    let OwnerState::Transaction(status) = d.owner else {
        return None;
    };
    let steps = match status.state() {
        TransactionState::Active
            if d.chain.groups == 1 && d.chain.apply == d.chain.birth && d.chain.previous == 0 =>
        {
            2
        }
        TransactionState::Aborting(RollbackState::Transaction { next: None })
            if d.chain.groups == 2
                && d.chain.previous == d.chain.birth
                && d.chain.apply > d.chain.birth =>
        {
            1
        }
        TransactionState::Aborted
            if allow_terminal
                && d.chain.groups == 3
                && d.chain.previous > d.chain.birth
                && d.chain.apply > d.chain.previous =>
        {
            0
        }
        _ => return None,
    };
    let mut remaining = [RemainingBudget::default(); 5];
    if steps >= 1 {
        remaining[2] = begin::empty_step_budget();
    }
    if steps == 2 {
        remaining[3] = begin::empty_step_budget();
    }
    if status.undo_head().is_some()
        || d.chain.birth == 0
        || d.chain.apply > prefix.complete_lsn
        || d.minimum_wal_lsn != d.chain.birth
        || d.inventory.change_lsn != d.chain.birth
        || d.inventory.items != 0
        || d.inventory.growth_extents != 0
        || d.inventory.digest != content_digest(&[])
        || d.last_mutation_sequence != 0
        || d.consumed_control != 0
        || d.consumed_wal != (3 - steps) * FRAME_BYTES as u64
        || d.remaining != remaining
    {
        return None;
    }
    Some((actor, location, d, steps))
}
fn ledger(bank: &[u8], profile: Profile, prefix: WalPrefix, adopted: u64) -> Option<Ledger> {
    let mut value = Ledger {
        occupied: 0,
        free: None,
        steps: 0,
        wal: 0,
    };
    for index in 0..32 {
        let r = row(bank, index);
        if r[PHASE] == 0 {
            if r.iter().any(|b| *b != 0) {
                return None;
            }
            value.free = value.free.or(Some(index));
            continue;
        }
        if r[PHASE] != STABLE {
            return None;
        }
        let (actor, location, _, steps) = descriptor(r, profile, prefix, false)?;
        if actor.id().local > adopted {
            return None;
        }
        for previous in 0..index {
            let other = row(bank, previous);
            if other[PHASE] != 0
                && (TxnRef::decode(&other[ACTOR]).ok()?.id() == actor.id()
                    || StatusSlotRef::decode(&other[LOCATION]).ok()?.page() == location.page())
            {
                return None;
            }
        }
        value.occupied |= 1 << index;
        value.steps = value.steps.checked_add(steps)?;
        value.wal = value.wal.checked_add(steps * FRAME_BYTES as u64)?;
    }
    Some(value)
}

impl<'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    EmptyTransactions<'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    fn health(&self) -> Result<()> {
        let storage = self.inner.inner.inner.storage;
        storage
            .health()
            .map_err(|e| fatal(storage, Cause::Storage(e)))?;
        if self.pending.is_some() {
            return Err(fatal(storage, Cause::Invariant));
        }
        let control = &self.inner.inner.inner.control;
        if control.installation.is_some()
            || control.base.installation.is_some()
            || control.base.batch.is_some()
            || control.base.terminal_pending
            || !super::ready(&self.inner.inner.flush)
            || !self
                .inner
                .inner
                .inner
                .pool
                .handoff_quiescent()
                .map_err(frame)?
        {
            return Err(unchanged(Cause::Busy));
        }
        if !fits::<W>() {
            return Err(unchanged(Cause::Capacity));
        }
        Ok(())
    }
    fn allocator_snapshot(
        &mut self,
        owner: &mut Owner<'c, 's>,
        prefix: WalPrefix,
    ) -> Result<FrameStamp> {
        let bound = &mut self.inner.inner.inner;
        let storage = bound.storage;
        let request = bound.pool.target_request(0).map_err(frame)?;
        let read = bound
            .pool
            .reserve_target_set(owner, &[request])
            .map_err(frame)?
            .try_read()
            .map_err(frame)?;
        let result = (|| {
            let stamp = read
                .copy_snapshot(
                    0,
                    (&mut bound.control.base.operation[65536..81920])
                        .try_into()
                        .expect("allocator scratch"),
                )
                .map_err(frame)?;
            bound
                .control
                .base
                .frozen_observation(storage.profile(), prefix, &stamp, self.frozen_system_lsn)
                .map_err(|e| fatal(storage, Cause::Allocation(e)))?;
            Ok(stamp)
        })();
        merge(result, read.release_checked().map_err(frame))
    }
    fn read_status(
        &mut self,
        owner: &mut Owner<'c, 's>,
        request: TargetRequest,
    ) -> Result<Option<FrameStamp>> {
        let bound = &mut self.inner.inner.inner;
        let storage = bound.storage;
        if let Some(actual) = bound.pool.resident_request(request).map_err(frame)? {
            let read = bound
                .pool
                .reserve_target_set(owner, &[actual])
                .map_err(frame)?
                .try_read()
                .map_err(frame)?;
            let result = read
                .copy_snapshot(0, bound.control.status_after)
                .map(Some)
                .map_err(frame);
            return merge(result, read.release_checked().map_err(frame));
        }
        // Exclusive outer borrow freezes allocation/residency. No pin or latch
        // survives into the selected home I/O. The caller rechecks the prefix.
        let control = &mut bound.control;
        let (root, scratch) = control.base.operation[..32768].split_at_mut(PAGE_BYTES);
        let read = (|| -> std::result::Result<(), PageIoError> {
            let mut reader = storage.core.open_reader(
                root.try_into().expect("root scratch"),
                scratch.try_into().expect("read scratch"),
            )?;
            reader
                .read_page(
                    request.identity,
                    request.kind,
                    request.level,
                    control.status_after,
                )
                .map(|_| ())
        })();
        read.map_err(|source| {
            let retryable = matches!(&source, PageIoError::Read(e) if e.effect() == Effect::Unchanged
                && e.certainty() == Certainty::Known
                && e.io_kind.is_some_and(|kind| kind != std::io::ErrorKind::UnexpectedEof))
                || matches!(&source, PageIoError::Namespace(e) if e.kind == NamespaceErrorKind::Io && e.effect == Effect::Unchanged);
            if retryable && !storage.session_fence().is_fenced() { unchanged(Cause::PageIo(source)) }
            else { fatal(storage, Cause::PageIo(source)) }
        })?;
        Ok(None)
    }
    fn census(
        &mut self,
        owner: &mut Owner<'c, 's>,
        prefix: WalPrefix,
        selected: PageRef,
        occupied: u32,
    ) -> Result<Selection> {
        self.check_directory(owner, prefix, Some(selected), occupied)?
            .ok_or_else(|| unchanged(Cause::Membership))
    }
    fn check_directory(
        &mut self,
        owner: &mut Owner<'c, 's>,
        prefix: WalPrefix,
        selected: Option<PageRef>,
        occupied: u32,
    ) -> Result<Option<Selection>> {
        let storage = self.inner.inner.inner.storage;
        let allocator_stamp = self.allocator_snapshot(owner, prefix)?;
        let mut ids = [None; 63];
        let mut count = 0;
        let mut seen = 0u32;
        let mut selection = None;
        for index in 1..64 {
            let entry = {
                let base = &self.inner.inner.inner.control.base;
                allocation_page::decode_allocation_page(
                    base.before(),
                    base.identity,
                    storage.profile().spec().namespace,
                )
                .and_then(|p| p.entry(index))
                .map_err(|_| fatal(storage, Cause::Invariant))?
            };
            let AllocationEntry::Allocated {
                generation,
                object,
                object_generation,
                kind,
            } = entry
            else {
                continue;
            };
            let request = TargetRequest {
                identity: PageIdentity {
                    database: storage.profile().spec().namespace.database,
                    page: PageRef {
                        file: allocator_stamp.identity.page.file,
                        page_no: index as u64,
                        incarnation: generation,
                    },
                    owner: object,
                    owner_generation: object_generation,
                },
                kind,
                level: 0,
            };
            let stamp = self.read_status(owner, request)?;
            let home = {
                let bytes = &*self.inner.inner.inner.control.status_after;
                let lsn = super::super::super::super::operation::status_envelope(
                    bytes,
                    &request.identity,
                )
                .map_err(|_| fatal(storage, Cause::Invariant))?;
                if lsn.0 == 0
                    || lsn.0 > prefix.synced_lsn
                    || stamp.is_some_and(|s| s.page_lsn != lsn)
                {
                    return Err(fatal(storage, Cause::Invariant));
                }
                let page = decode_status_home_page(
                    bytes,
                    request.identity,
                    storage.profile().spec().namespace,
                )
                .map_err(|e| fatal(storage, Cause::Page(e)))?;
                let home = page.cell(0).map_err(|e| fatal(storage, Cause::Page(e)))?;
                if home.incoming() != 0 {
                    return Err(fatal(storage, Cause::Invariant));
                }
                match home.core() {
                    TransactionStatusCell::Free(g)
                        if g.get() == 1 && lsn.0 <= self.frozen_system_lsn.0 => {}
                    TransactionStatusCell::Occupied(status) => {
                        let actor = status.reference();
                        if actor.id().writer_epoch != prefix.stream.writer_epoch
                            || actor.id().local > self.adopted_local
                            || actor.generation().get() != 2
                            || status.undo_head().is_some()
                            || ids[..count].contains(&Some(actor.id()))
                        {
                            return Err(fatal(storage, Cause::Invariant));
                        }
                        ids[count] = Some(actor.id());
                        count += 1;
                        let mut matched = None;
                        for bank_index in 0..32 {
                            let r = row(self.bank, bank_index);
                            if r[PHASE] != 0
                                && TxnRef::decode(&r[ACTOR])
                                    .map_err(|e| fatal(storage, Cause::Page(e)))?
                                    == actor
                            {
                                matched = Some(bank_index);
                            }
                        }
                        if status.state() == TransactionState::Aborted {
                            if matched.is_some() {
                                return Err(fatal(storage, Cause::Invariant));
                            }
                        } else {
                            let bank_index =
                                matched.ok_or_else(|| fatal(storage, Cause::Invariant))?;
                            let (_, location, d, _) = descriptor(
                                row(self.bank, bank_index),
                                storage.profile(),
                                prefix,
                                false,
                            )
                            .ok_or_else(|| fatal(storage, Cause::Invariant))?;
                            if location.page() != request.identity.page
                                || location.slot() != 0
                                || d.owner != OwnerState::Transaction(status)
                                || d.chain.apply != lsn.0
                                || seen & (1 << bank_index) != 0
                            {
                                return Err(fatal(storage, Cause::Invariant));
                            }
                            seen |= 1 << bank_index;
                        }
                    }
                    _ => return Err(fatal(storage, Cause::Invariant)),
                }
                home
            };
            if Some(request.identity.page) == selected {
                if let Some(stamp) = stamp {
                    let control = &mut self.inner.inner.inner.control;
                    control.base.operation[81920..98304].copy_from_slice(control.status_after);
                    selection = Some(Selection {
                        request,
                        stamp,
                        home,
                    });
                } else {
                    return Err(unchanged(Cause::NotResident));
                }
            }
        }
        if seen != occupied {
            #[cfg(test)]
            tests::census_mismatch(seen, occupied);
            return Err(fatal(storage, Cause::Invariant));
        }
        if self.allocator_snapshot(owner, prefix)? != allocator_stamp
            || storage
                .wal()
                .prefix()
                .map_err(|e| fatal(storage, Cause::Wal(e)))?
                != prefix
        {
            return Err(fatal(storage, Cause::Invariant));
        }
        Ok(selection)
    }
    /// A healthy candidate consumes local+1 even if a later admission fails.
    /// Unchanged describes WAL/page/reservation effects, not that issued counter.
    pub(crate) fn begin(
        &mut self,
        owner: &mut Owner<'c, 's>,
        page: PageRef,
    ) -> Result<EmptyTransactionObservation> {
        self.change(owner, Change::Begin, Some(page), None)
    }
    pub(crate) fn rollback_start(
        &mut self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
    ) -> Result<EmptyTransactionObservation> {
        self.change(owner, Change::Rollback, None, Some(actor))
    }
    pub(crate) fn abort_end(
        &mut self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
    ) -> Result<EmptyTransactionObservation> {
        self.change(owner, Change::Abort, None, Some(actor))
    }
    fn change(
        &mut self,
        owner: &mut Owner<'c, 's>,
        change: Change,
        page: Option<PageRef>,
        actor: Option<TxnRef>,
    ) -> Result<EmptyTransactionObservation> {
        let storage = self.inner.inner.inner.storage;
        let _unwind = storage.session_fence().unwind_guard();
        self.health()?;
        if !owner
            .check_binding(self.inner.inner.inner.pool.bound_core())
            .map_err(|e| frame(e.into()))?
        {
            return Err(unchanged(Cause::BindingMismatch));
        }
        let previous_issued = self.issued_local;
        if change == Change::Begin {
            self.issued_local = self
                .issued_local
                .checked_add(1)
                .ok_or_else(|| unchanged(Cause::Exhausted))?;
        }
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|e| fatal(storage, Cause::Wal(e)))?;
        let ledger = ledger(self.bank, storage.profile(), prefix, self.adopted_local)
            .ok_or_else(|| fatal(storage, Cause::Invariant))?;
        let index = if change == Change::Begin {
            ledger.free.ok_or_else(|| unchanged(Cause::Capacity))?
        } else {
            (0..32)
                .find(|&i| {
                    row(self.bank, i)[PHASE] == STABLE
                        && TxnRef::decode(&row(self.bank, i)[ACTOR]).ok() == actor
                })
                .ok_or_else(|| unchanged(Cause::MissingTransaction))?
        };
        let page = match page {
            Some(page) => page,
            None => StatusSlotRef::decode(&row(self.bank, index)[LOCATION])
                .map_err(|e| fatal(storage, Cause::Page(e)))?
                .page(),
        };
        let selection = self.census(owner, prefix, page, ledger.occupied)?;
        let (actor, location, steps) = if change == Change::Begin {
            let TransactionStatusCell::Free(g) = selection.home.core() else {
                return Err(unchanged(Cause::WrongState));
            };
            let generation = Generation::new(
                g.get()
                    .checked_add(1)
                    .ok_or_else(|| unchanged(Cause::Exhausted))?,
            )
            .map_err(|_| unchanged(Cause::Exhausted))?;
            let actor = TxnRef::new(
                TxnId {
                    database: prefix.stream.database,
                    timeline: prefix.stream.timeline,
                    writer_epoch: prefix.stream.writer_epoch,
                    local: self.issued_local,
                },
                generation,
            )
            .map_err(|e| fatal(storage, Cause::Page(e)))?;
            (
                actor,
                StatusSlotRef::new(page, 0, generation)
                    .map_err(|e| fatal(storage, Cause::Page(e)))?,
                3,
            )
        } else {
            let (actor, location, d, steps) =
                descriptor(row(self.bank, index), storage.profile(), prefix, false)
                    .ok_or_else(|| fatal(storage, Cause::Invariant))?;
            let required = if change == Change::Rollback {
                TransactionState::Active
            } else {
                TransactionState::Aborting(RollbackState::Transaction { next: None })
            };
            let OwnerState::Transaction(status) = d.owner else {
                return Err(fatal(storage, Cause::Invariant));
            };
            if status.state() != required {
                return Err(unchanged(Cause::WrongState));
            }
            (actor, location, steps)
        };
        admit(prefix, ledger, selection.stamp, change, steps)?;
        // Census's bounded directory and reader/file values have ended here.
        let control = &mut self.inner.inner.inner.control;
        let pool = &self.inner.inner.inner.pool;
        let (work, rest) = control.base.operation.split_at_mut(81920);
        let before: &[u8; PAGE_BYTES] = (&*rest).try_into().expect("status before scratch");
        let output: &mut [u8; FRAME_BYTES] = (&mut work[..FRAME_BYTES])
            .try_into()
            .expect("frame scratch");
        let expected = RollbackStartExpected {
            frame: FrameExpectation {
                stream: prefix.stream,
                previous_lsn: prefix.complete_lsn,
            },
            profile: storage.profile(),
            actor,
            location,
            page: selection.request.identity,
        };
        let lsn = prefix.complete_lsn + 1; // aggregate completion headroom checked
        let header = match change {
            Change::Begin => begin::produce_begin(
                begin::BeginExpected {
                    frame: expected.frame,
                    profile: expected.profile,
                    actor,
                    location,
                    page: expected.page,
                    previous_local: previous_issued,
                },
                lsn,
                before,
                output,
                control.status_after,
            )
            .map(|v| v.header()),
            Change::Rollback => semantic_group::produce_rollback_start(
                expected,
                lsn,
                &row(self.bank, index)[..1536],
                before,
                output,
                control.status_after,
            )
            .map(|v| v.header()),
            Change::Abort => empty_abort::produce_empty_abort(
                expected,
                lsn,
                &row(self.bank, index)[..1536],
                before,
                output,
                control.status_after,
            )
            .map(|v| v.header()),
        }
        .map_err(|e| fatal(storage, Cause::Semantic(e)))?;
        // These immutable loans stay sealed until common installation completes.
        let bytes = &*output;
        let after = &*control.status_after;
        let mut location_bytes = [0; 40];
        let mut actor_bytes = [0; 48];
        location
            .encode(&mut location_bytes)
            .map_err(|e| fatal(storage, Cause::Page(e)))?;
        actor
            .encode(&mut actor_bytes)
            .map_err(|e| fatal(storage, Cause::Page(e)))?;
        let mut write = pool
            .reserve_target_set(owner, &[selection.request])
            .map_err(frame)?
            .try_write()
            .map_err(frame)?;
        let installed = (|| {
            write.matches_stamp(0, &selection.stamp).map_err(frame)?;
            let dpt = write.preflight_dpt(0, &selection.stamp).map_err(frame)?;
            let requirement = storage
                .wal()
                .stage_verified_required(prefix, header, bytes)
                .map_err(|e| EmptyTransactionError {
                    effect: e.effect,
                    cause: Cause::Wal(e),
                })?;
            // No fallible operation or callback between stage and retained owner.
            self.pending = Some(requirement);
            let bank_row = row_mut(self.bank, index);
            bank_row[LOCATION].copy_from_slice(&location_bytes);
            bank_row[ACTOR].copy_from_slice(&actor_bytes);
            bank_row[PHASE] = STAGED;
            if change == Change::Begin {
                self.adopted_local = actor.id().local;
            }
            #[cfg(test)]
            tests::hit(STAGED, storage);
            let permit = DirtyInstallPermit::from_empty_transaction(EmptyTransactionInstall {
                core: pool.bound_core(),
                stamp: selection.stamp,
                requirement,
                after,
            });
            write.install_page(0, &permit).map_err(frame)?;
            bank_row[PHASE] = PAGE_INSTALLED;
            #[cfg(test)]
            tests::hit(PAGE_INSTALLED, storage);
            write.install_metadata(0, &permit).map_err(frame)?;
            bank_row[..1536].copy_from_slice(&bytes[352..1888]);
            bank_row[PHASE] = DESCRIPTOR_INSTALLED;
            #[cfg(test)]
            tests::hit(DESCRIPTOR_INSTALLED, storage);
            write.install_dpt(dpt, &permit).map_err(frame)?;
            bank_row[PHASE] = DPT_INSTALLED;
            #[cfg(test)]
            tests::hit(DPT_INSTALLED, storage);
            Ok(())
        })();
        let installed = installed.map_err(|e: EmptyTransactionError| {
            if self.pending.is_some() {
                fatal(storage, e.cause)
            } else {
                e
            }
        });
        merge(installed, write.release_checked().map_err(frame)).map_err(|e| {
            if self.pending.is_some() {
                fatal(storage, e.cause)
            } else {
                e
            }
        })?;
        let requirement = self.pending.expect("actual complete stage retained");
        let durable = storage
            .wal()
            .wait_durable(&requirement)
            .map_err(|e| fatal(storage, Cause::Wal(e)))?;
        if !durable.confirms(&requirement) {
            return Err(fatal(storage, Cause::Invariant));
        }
        #[cfg(test)]
        tests::after_sync(self);
        self.finish(
            owner,
            index,
            actor,
            location,
            ledger,
            change,
            requirement,
            selection.stamp,
        )
    }
    #[allow(clippy::too_many_arguments)] // Exact pre-stage values remain separate, Copy witnesses.
    fn finish(
        &mut self,
        owner: &mut Owner<'c, 's>,
        index: usize,
        actor: TxnRef,
        location: StatusSlotRef,
        before_ledger: Ledger,
        change: Change,
        requirement: WalRequirement<'s>,
        before_stamp: FrameStamp,
    ) -> Result<EmptyTransactionObservation> {
        let storage = self.inner.inner.inner.storage;
        storage
            .health()
            .map_err(|e| fatal(storage, Cause::Storage(e)))?;
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|e| fatal(storage, Cause::Wal(e)))?;
        if self.pending.map(|value| value.location()) != Some(requirement.location())
            || !self
                .pending
                .is_some_and(|value| value.uses_fence(storage.session_fence()))
            || !requirement.uses_fence(storage.session_fence())
            || prefix.complete_offset != requirement.location().end
            || prefix.complete_lsn != requirement.location().lsn
            || prefix.synced_lsn != prefix.complete_lsn
            || prefix.synced_offset != prefix.complete_offset
            || row(self.bank, index)[PHASE] != DPT_INSTALLED
        {
            return Err(fatal(storage, Cause::Invariant));
        }
        let (_, actual_location, descriptor, steps) =
            descriptor(row(self.bank, index), storage.profile(), prefix, true)
                .ok_or_else(|| fatal(storage, Cause::Invariant))?;
        let OwnerState::Transaction(status) = descriptor.owner else {
            return Err(fatal(storage, Cause::Invariant));
        };
        let role = &self.inner.inner.inner.control.role;
        let request = TargetRequest {
            identity: PageIdentity {
                database: role.namespace().database,
                page: location.page(),
                owner: role.object(),
                owner_generation: role.generation(),
            },
            kind: PageKind::TransactionStatus,
            level: 0,
        };
        let stamp = self
            .read_status(owner, request)
            .map_err(|e| fatal(storage, e.cause))?
            .ok_or_else(|| fatal(storage, Cause::Invariant))?;
        let page = decode_status_home_page(
            self.inner.inner.inner.control.status_after,
            request.identity,
            storage.profile().spec().namespace,
        )
        .map_err(|e| fatal(storage, Cause::Page(e)))?;
        let expected_state = match change {
            Change::Begin => TransactionState::Active,
            Change::Rollback => {
                TransactionState::Aborting(RollbackState::Transaction { next: None })
            }
            Change::Abort => TransactionState::Aborted,
        };
        if row(self.bank, index)[..1536] != self.inner.inner.inner.control.base.operation[352..1888]
            || status.state() != expected_state
            || stamp.identity != before_stamp.identity
            || stamp.generation != before_stamp.generation
            || before_stamp.modification_counter.checked_add(1) != Some(stamp.modification_counter)
            || stamp.rec_lsn
                != Some(
                    before_stamp
                        .rec_lsn
                        .unwrap_or(PageLsn(requirement.location().lsn)),
                )
            || actual_location != location
            || status.reference() != actor
            || descriptor.chain.apply != requirement.location().lsn
            || stamp.page_lsn.0 != descriptor.chain.apply
            || page.lsn() != stamp.page_lsn.0
            || page
                .lookup(actor, location)
                .map_err(|e| fatal(storage, Cause::Page(e)))?
                != StatusHome::new(TransactionStatusCell::Occupied(status), 0)
                    .map_err(|e| fatal(storage, Cause::Page(e)))?
        {
            return Err(fatal(storage, Cause::Invariant));
        }
        self.allocator_snapshot(owner, prefix)
            .map_err(|e| fatal(storage, e.cause))?;
        storage
            .health()
            .map_err(|e| fatal(storage, Cause::Storage(e)))?;
        if storage
            .wal()
            .prefix()
            .map_err(|e| fatal(storage, Cause::Wal(e)))?
            != prefix
        {
            return Err(fatal(storage, Cause::Invariant));
        }
        let remaining_wal = if change == Change::Begin {
            before_ledger.wal + steps * FRAME_BYTES as u64
        } else {
            before_ledger.wal - FRAME_BYTES as u64
        };
        let retained_transactions = before_ledger.occupied.count_ones() as usize
            + usize::from(change == Change::Begin)
            - usize::from(change == Change::Abort);
        // Only this exact staged row may be terminal while retained. Its durable
        // page and system checks completed above; stable census forbids it.
        if change == Change::Abort {
            row_mut(self.bank, index).fill(0);
        } else {
            row_mut(self.bank, index)[PHASE] = STABLE;
        }
        self.pending = None;
        Ok(EmptyTransactionObservation {
            actor,
            location,
            state: status.state(),
            lsn: stamp.page_lsn,
            issued_local: self.issued_local,
            adopted_local: self.adopted_local,
            retained_transactions,
            remaining_wal,
        })
    }
}
fn admit(
    prefix: WalPrefix,
    ledger: Ledger,
    stamp: FrameStamp,
    change: Change,
    steps: u64,
) -> Result<()> {
    let total_steps = ledger
        .steps
        .checked_add(if change == Change::Begin { 3 } else { 0 })
        .ok_or_else(|| unchanged(Cause::Capacity))?;
    prefix
        .complete_lsn
        .checked_add(total_steps)
        .ok_or_else(|| unchanged(Cause::Exhausted))?;
    stamp
        .modification_counter
        .checked_add(steps)
        .ok_or_else(|| unchanged(Cause::Exhausted))?;
    let additional = ledger
        .wal
        .checked_add(if change == Change::Begin {
            3 * FRAME_BYTES as u64
        } else {
            0
        })
        .ok_or_else(|| unchanged(Cause::Capacity))?;
    let future = prefix
        .complete_offset
        .checked_add(additional)
        .filter(|v| *v <= crate::storage::wal_io::WAL_FILE_CAP)
        .ok_or_else(|| unchanged(Cause::Capacity))?;
    // Conservative full first-extent image protection, even when homes are clean.
    crate::storage::wal_io::bootstrap::PROTECTED_BYTES
        .checked_add(future - 4096)
        .and_then(|v| v.checked_add(64 * PAGE_BYTES as u64))
        .filter(|v| *v <= 1 << 30)
        .ok_or_else(|| unchanged(Cause::Capacity))?;
    Ok(())
}

// Source-value census only. All 32 encoded descriptors are retained bank bytes;
// at most one decoded row is kept. The 63-ID array dies when census returns.
fn call_bytes<W: WriterNamespace>() -> usize {
    // RollbackStart uses the same Expected/Observation layout and the same
    // sequential produce/build/materialize/observe chain as empty Abort. Its
    // 256-byte operation core and budget array are covered by Abort's larger
    // descriptor/cell counts and 6144-byte encoder allowance.
    let codec = begin::component_bytes().max(empty_abort::component_bytes());
    let common = 4 * size_of::<Selection>()
        + 4 * size_of::<Ledger>()
        + 4 * size_of::<WalPrefix>()
        + 4 * size_of::<RollbackStartExpected>()
        + 4 * size_of::<Result<EmptyTransactionObservation>>()
        + 4 * size_of::<WalFrameHeader>()
        + 2 * size_of::<EmptyTransactionInstall<'_, '_, '_>>()
        + 2 * size_of::<DirtyInstallPermit<'_, '_, '_>>()
        + 2048;
    let prepare = codec + common;
    let census = 63 * size_of::<Option<TxnId>>()
        + 4 * size_of::<DescriptorSpec>()
        + common
        + 2 * size_of::<PageFileReader<'_, W>>()
        + 4 * size_of::<W::File>()
        + super::super::super::super::budget::TEMPORARIES
            .max(super::super::super::super::budget::CONSTRUCTOR)
        + dirty_frame::target_set_call_bytes::<1>();
    prepare
        .max(census)
        .max(common + dirty_frame::target_set_call_bytes::<1>() + 8192)
}
fn fits<W: WriterNamespace>() -> bool {
    call_bytes::<W>() <= 65536
        && super::retained_bytes::<W>() + call_bytes::<W>() <= 196608
        && super::retained_bytes::<W>() + call_bytes::<W>() + 65536 <= 262144
        && super::handoff_fits::<W>()
}

#[cfg(test)]
#[path = "live_tests.rs"]
mod tests;

#[path = "history.rs"]
mod history;
pub(in super::super::super) use history::History;

impl<'s, 'c, W: WriterNamespace> EmptyTransactions<'_, 's, 'c, '_, '_, '_, '_, '_, W> {
    fn historical_images(
        &mut self,
        owner: &mut Owner<'c, 's>,
        page: PageRef,
    ) -> Result<(Selection, history::History)> {
        let storage = self.inner.inner.inner.storage;
        let _unwind = storage.session_fence().unwind_guard();
        self.health()?;
        if !owner
            .check_binding(self.inner.inner.inner.pool.bound_core())
            .map_err(|e| frame(e.into()))?
        {
            return Err(unchanged(Cause::BindingMismatch));
        }
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|e| fatal(storage, Cause::Wal(e)))?;
        let ledger = ledger(self.bank, storage.profile(), prefix, self.adopted_local)
            .ok_or_else(|| fatal(storage, Cause::Invariant))?;
        let selection = self.census(owner, prefix, page, ledger.occupied)?;
        let digest = content_digest(&self.inner.inner.inner.control.base.operation[81920..98304]);
        let history = self.scan_history(prefix, selection.request, digest)?;
        Ok((selection, history))
    }
    fn scan_history(
        &mut self,
        prefix: WalPrefix,
        request: TargetRequest,
        digest: [u8; 32],
    ) -> Result<history::History> {
        let storage = self.inner.inner.inner.storage;
        let allocator_request = self
            .inner
            .inner
            .inner
            .pool
            .target_request(0)
            .map_err(frame)?;
        let allocator_digest =
            content_digest(&self.inner.inner.inner.control.base.operation[65536..81920]);
        let descriptor = (0..32).find_map(|i| {
            let r = row(self.bank, i);
            (r[PHASE] == STABLE
                && StatusSlotRef::decode(&r[LOCATION])
                    .ok()
                    .is_some_and(|location| location.page() == request.identity.page))
            .then_some(&r[..1536])
        });
        let result = {
            // All pool pins and content latches were released by census. The
            // original WAL handle remains protected from concurrent write here.
            let mut source = match storage.wal().file.try_lock() {
                Ok(file) => file,
                Err(std::sync::TryLockError::WouldBlock) => return Err(unchanged(Cause::Busy)),
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    return Err(fatal(storage, Cause::Invariant));
                }
            };
            history::reconstruct(
                &mut *source,
                self.inner.inner.inner.control.base.operation,
                self.inner.inner.inner.control.status_after,
                storage.profile(),
                allocator_request.identity,
                allocator_digest,
                descriptor,
                prefix,
                self.frozen_system_lsn,
                request,
                digest,
            )
        };
        let history = result.map_err(|e| {
            // EOF inside the retained complete prefix proves source drift.
            // A count-contract violation has no OS category and cannot certify
            // a retryable read either. Ordinary unchanged I/O remains retryable.
            if matches!(&e.cause, Cause::HistoryIo(io) if io.effect() == Effect::Unchanged
                && io.certainty() == Certainty::Known
                && io.io_kind.is_some_and(|kind| kind != std::io::ErrorKind::UnexpectedEof))
                && !storage.session_fence().is_fenced()
            {
                e
            } else {
                fatal(storage, e.cause)
            }
        })?;
        storage
            .health()
            .map_err(|e| fatal(storage, Cause::Storage(e)))?;
        if storage
            .wal()
            .prefix()
            .map_err(|e| fatal(storage, Cause::Wal(e)))?
            != prefix
        {
            return Err(fatal(storage, Cause::Invariant));
        }
        Ok(history)
    }
}

impl<'s, 'c, W: WriterNamespace> EmptyTransactions<'_, 's, 'c, '_, '_, '_, '_, '_, W> {
    /// Flush a resident live Status through the original DWB/retirement owner.
    /// Historical home bytes must match this page's actual complete WAL history.
    pub(crate) fn flush_status(
        &mut self,
        owner: &mut Owner<'c, 's>,
        page: PageRef,
    ) -> Result<crate::storage::wal_io::FlushProgress> {
        let storage = self.inner.inner.inner.storage;
        let _unwind = storage.session_fence().unwind_guard();
        if !history_fits::<W>() {
            return Err(unchanged(Cause::Capacity));
        }
        let (_, history) = self.historical_images(owner, page)?;
        let (_, _, mut flusher) = self.inner.inner.split_with_flush();
        flusher
            .try_flush_history(owner, page, history)
            .map_err(|e| EmptyTransactionError {
                effect: e.effect,
                cause: Cause::Flush(e),
            })
    }
    pub(crate) fn continue_status_flush(
        &mut self,
        owner: &mut Owner<'c, 's>,
    ) -> Result<crate::storage::wal_io::FlushProgress> {
        let (_, _, mut flusher) = self.inner.inner.split_with_flush();
        flusher
            .continue_flush(owner)
            .map_err(|e| EmptyTransactionError {
                effect: e.effect,
                cause: Cause::Flush(e),
            })
    }
}
fn history_call_bytes<W: WriterNamespace>() -> usize {
    // Census returns before history scan. Pages/descriptor use the existing
    // loan. The selected semantic materializer is the largest nested phase.
    let scan = begin::component_bytes().max(empty_abort::component_bytes())
        + 4 * size_of::<history::History>()
        + 4 * size_of::<Selection>()
        + 4 * size_of::<WalPrefix>()
        + 4 * size_of::<FrameExpectation>()
        + 4 * size_of::<RollbackStartExpected>()
        + 4 * size_of::<W::File>()
        + 8192;
    let system = super::super::super::super::operation::plan_bytes()
        + super::super::super::super::operation::temporary_bytes()
        + 8192;
    let bootstrap = crate::storage::wal_io::bootstrap::recovery_preparation_bytes::<W>() + 2048;
    call_bytes::<W>().max(scan).max(system).max(bootstrap)
}
fn history_fits<W: WriterNamespace>() -> bool {
    history_call_bytes::<W>() <= 65536
        && super::retained_bytes::<W>() + history_call_bytes::<W>() <= 196608
        && super::retained_bytes::<W>() + history_call_bytes::<W>() + 65536 <= 262144
        && super::handoff_fits::<W>()
}

#[path = "load.rs"]
mod load;
pub(in super::super::super) use load::EmptyLoadInstall;

#[path = "pressure.rs"]
mod pressure;
