//! Consuming handoff from Status provisioning to the empty-transaction owner.
//! This first boundary freezes system allocation and retains caller-owned space.
//! Live empty transitions retain the same pool; recovery remains a separate owner.

use super::{ResidentStatus, ready};
use crate::{
    storage::{
        allocation_page::{self, AllocationEntry},
        dirty_frame::{self, FrameStamp},
        page::PageLsn,
        wal_io::{MutableStorageError, WalIoError, WalPrefix},
    },
    transaction::concurrency::Owner,
};
use std::{mem::size_of, sync::TryLockError};
use xadb_foundation::{error::Effect, namespace::WriterNamespace};

/// Thirty-two retained slots, each with a 1536-byte descriptor and 128 bytes
/// reserved for private location/state bookkeeping. This is not a disk format.
/// All bytes are charged as retained metadata even while no slot is occupied.
pub(crate) const EMPTY_TRANSACTION_BANK_BYTES: usize = 32 * 1664;

#[derive(Debug)]
pub(crate) enum Cause {
    Busy,
    Capacity,
    NoStatus,
    BindingMismatch,
    Invariant,
    Storage(MutableStorageError),
    Frame(dirty_frame::DirtyError),
    Wal(WalIoError),
    Allocation(super::super::super::dirty_allocation::Cause),
    Page(crate::storage::page::PageError),
    PageIo(crate::storage::page_io::PageIoError),
    Semantic(crate::storage::semantic_group::SemanticError),
    Membership,
    MissingTransaction,
    WrongState,
    NotResident,
    AlreadyResident,
    HistoryIo(xadb_foundation::Error),
    Flush(super::super::StatusFlushError),
    Exhausted,
}
#[derive(Debug)]
pub(crate) struct EmptyTransactionError {
    pub(crate) cause: Cause,
    pub(crate) effect: Effect,
}
type Result<T> = std::result::Result<T, EmptyTransactionError>;

/// Before returns the original provisioning owner. Its private scratch may
/// have been overwritten, but pages, descriptors, WAL and the bank are intact.
/// Fatal never returns an owner advertised as reusable. The Result's success
/// branch retains the bank borrow until the caller consumes/drops that Result.
pub(crate) enum IntoEmptyTransactionsError<B> {
    Before {
        error: EmptyTransactionError,
        bound: B,
    },
    Fatal(EmptyTransactionError),
}

pub(crate) struct EmptyTransactions<'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> {
    // Preserve the original binding's Drop/fence and flush-workspace lifetime.
    // No accessor, split or reverse conversion exposes this provisioning owner.
    inner: ResidentStatus<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    bank: &'b mut [u8; EMPTY_TRANSACTION_BANK_BYTES],
    frozen_system_lsn: PageLsn,
    issued_local: u64,
    adopted_local: u64,
    pending: Option<crate::storage::wal_io::WalRequirement<'s>>,
}

/// Diagnostics only: neither the bank nor the system descriptor is exported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EmptyTransactionSnapshot {
    pub(crate) frozen_system_lsn: PageLsn,
    pub(crate) issued_local: u64,
    pub(crate) adopted_local: u64,
    pub(crate) retained_bank_bytes: usize,
}
impl<W: WriterNamespace> EmptyTransactions<'_, '_, '_, '_, '_, '_, '_, '_, W> {
    pub(crate) fn snapshot(&self) -> Result<EmptyTransactionSnapshot> {
        self.inner
            .inner
            .inner
            .storage
            .health()
            .map_err(|source| EmptyTransactionError {
                cause: Cause::Storage(source),
                effect: Effect::CallerMustResolve,
            })?;
        Ok(EmptyTransactionSnapshot {
            frozen_system_lsn: self.frozen_system_lsn,
            issued_local: self.issued_local,
            adopted_local: self.adopted_local,
            retained_bank_bytes: self.bank.len(),
        })
    }
}

fn unchanged(cause: Cause) -> EmptyTransactionError {
    EmptyTransactionError {
        cause,
        effect: Effect::Unchanged,
    }
}
fn frame(source: dirty_frame::DirtyError) -> EmptyTransactionError {
    EmptyTransactionError {
        effect: source.effect,
        cause: Cause::Frame(source),
    }
}

impl<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> ResidentStatus<'s, 'c, 'p, 'd, 'o, 'm, 'w, W> {
    /// Freeze the system branch after provisioning at least one Status page.
    /// Only the fresh, never-recovered binding can reach this type. It exposes
    /// INIT/read/flush/load, but no transaction issuer; there is no reverse
    /// conversion. Thus local zero is not inferred from current Free homes.
    /// No file write, sync, transaction ID or live reservation occurs here.
    #[allow(
        clippy::result_large_err,
        reason = "return the exact inline owner without allocating"
    )]
    pub(crate) fn into_empty_transactions<'b>(
        mut self,
        owner: &mut Owner<'c, 's>,
        bank: &'b mut [u8; EMPTY_TRANSACTION_BANK_BYTES],
    ) -> std::result::Result<
        EmptyTransactions<'b, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
        IntoEmptyTransactionsError<Self>,
    > {
        let () = ConversionLayout::<W>::CHECK;
        let storage = self.inner.inner.storage;
        let _unwind = storage.session_fence().unwind_guard();
        let prepared = self.prepare_empty_handoff(owner);
        match prepared {
            Ok(frozen_system_lsn) => {
                // prepare returned after checked pin/latch cleanup and health.
                // No fallible step or callback separates initialization/move.
                bank.fill(0);
                Ok(EmptyTransactions {
                    inner: self,
                    bank,
                    frozen_system_lsn,
                    issued_local: 0,
                    adopted_local: 0,
                    pending: None,
                })
            }
            Err(error) if error.effect == Effect::Unchanged => {
                Err(IntoEmptyTransactionsError::Before { error, bound: self })
            }
            Err(mut error) => {
                storage.session_fence().fence();
                error.effect = Effect::CallerMustResolve;
                Err(IntoEmptyTransactionsError::Fatal(error))
            }
        }
    }

    fn prepare_empty_handoff(&mut self, owner: &mut Owner<'c, 's>) -> Result<PageLsn> {
        let storage = self.inner.inner.storage;
        let fatal = |cause| {
            storage.session_fence().fence();
            EmptyTransactionError {
                cause,
                effect: Effect::CallerMustResolve,
            }
        };
        storage
            .health()
            .map_err(|source| fatal(Cause::Storage(source)))?;
        if !handoff_fits::<W>() {
            return Err(unchanged(Cause::Capacity));
        }
        let pool = &self.inner.inner.pool;
        let control = &self.inner.inner.control;
        if !pool.uses_storage_origin(&storage.binding, storage.session_fence())
            || !control.role.uses_fence(storage.session_fence())
            || control.role.namespace() != storage.profile().spec().namespace
        {
            return Err(fatal(Cause::BindingMismatch));
        }
        // try_lock is essential: mem::forget can end a Rust borrow while leaving
        // admission or pins retained. No blocking wait is hidden in conversion.
        match self.read_gate.try_lock() {
            Ok(gate) => drop(gate),
            Err(TryLockError::WouldBlock) => return Err(unchanged(Cause::Busy)),
            Err(TryLockError::Poisoned(error)) => {
                drop(error.into_inner());
                return Err(fatal(Cause::Invariant));
            }
        }
        if control.installation.is_some()
            || control.base.installation.is_some()
            || control.base.terminal_pending
            || control.base.batch.is_some()
            || !ready(&self.inner.flush)
            || !pool.handoff_quiescent().map_err(frame)?
        {
            return Err(unchanged(Cause::Busy));
        }
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|source| fatal(Cause::Wal(source)))?;
        let request = pool.target_request(0).map_err(frame)?;
        let read = pool
            .reserve_target_set(owner, &[request])
            .map_err(frame)?
            .try_read()
            .map_err(frame)?;
        let result = (|| {
            let base = &mut self.inner.inner.control.base;
            let stamp = read
                .copy_snapshot(
                    0,
                    (&mut base.operation[65536..81920])
                        .try_into()
                        .expect("fixed allocator scratch"),
                )
                .map_err(frame)?;
            base.observation(storage.profile(), prefix, &stamp, false)
                .map_err(|source| fatal(Cause::Allocation(source)))?;
            let allocation = allocation_page::decode_allocation_page(
                base.before(),
                base.identity,
                storage.profile().spec().namespace,
            )
            .map_err(|_| fatal(Cause::Invariant))?;
            let mut populated = false;
            for index in 1..64 {
                if matches!(
                    allocation
                        .entry(index)
                        .map_err(|_| fatal(Cause::Invariant))?,
                    AllocationEntry::Allocated { .. }
                ) {
                    populated = true;
                }
            }
            if !populated {
                return Err(unchanged(Cause::NoStatus));
            }
            Ok(stamp.page_lsn)
        })();
        let cleanup = read
            .release_checked()
            .map_err(|source| fatal(Cause::Frame(source)));
        let result = match (result, cleanup) {
            (Err(original), _) if original.effect == Effect::CallerMustResolve => Err(original),
            (_, Err(cleanup)) => Err(cleanup),
            (result, Ok(())) => result,
        };
        storage
            .health()
            .map_err(|source| fatal(Cause::Storage(source)))?;
        let current = storage
            .wal()
            .prefix()
            .map_err(|source| fatal(Cause::Wal(source)))?;
        if current != prefix {
            return Err(fatal(Cause::Invariant));
        }
        result
    }
}

// Source-value bounds, not compiler physical stack/RSS measurements. The bank
// is borrowed, never copied through an input, Result, or Before carrier. During
// checking, the input and caller's outer Result coexist with the complete pin /
// allocator / descriptor / hash chain. That helper returns before constructing
// the New or Before value, which is then moved into the outer Result. Charge the
// largest of those phases; do not sum mutually exclusive success/error values.
const fn conversion_bytes<W: WriterNamespace>() -> usize {
    type Old<'a, W> = ResidentStatus<'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    type New<'a, W> = EmptyTransactions<'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    type Error<'a, W> = IntoEmptyTransactionsError<Old<'a, W>>;
    let checking = dirty_frame::target_set_call_bytes::<1>()
        + super::super::super::budget::TEMPORARIES
        + 2 * size_of::<WalPrefix>()
        + 2 * size_of::<FrameStamp>()
        + 2 * size_of::<Result<PageLsn>>();
    let carrier = if size_of::<New<'_, W>>() > size_of::<Error<'_, W>>() {
        size_of::<New<'_, W>>()
    } else {
        size_of::<Error<'_, W>>()
    };
    let phase = if checking > carrier {
        checking
    } else {
        carrier
    };
    size_of::<Old<'_, W>>()
        + size_of::<std::result::Result<New<'_, W>, Error<'_, W>>>()
        + phase
        + 2048
}
struct ConversionLayout<W>(std::marker::PhantomData<W>);
impl<W: WriterNamespace> ConversionLayout<W> {
    const CHECK: () = assert!(conversion_bytes::<W>() <= 65536);
}
fn retained_bytes<W: WriterNamespace>() -> usize {
    // Same original retained controls/pool census as ResidentStatus. Replacing
    // its API removes wide read carriers; no transaction operation runs yet.
    60672
        + super::super::history_retained_bytes()
        + dirty_frame::init_pool_delta_bytes()
        + size_of::<super::super::super::StatusControl<'_, '_, '_>>()
            .saturating_sub(super::super::super::dirty_allocation::baseline_control_bytes())
        + size_of::<std::sync::Mutex<()>>()
        + size_of::<EmptyTransactions<'_, '_, '_, '_, '_, '_, '_, '_, W>>()
            .saturating_sub(size_of::<ResidentStatus<'_, '_, '_, '_, '_, '_, '_, W>>())
        + EMPTY_TRANSACTION_BANK_BYTES
}
fn handoff_fits<W: WriterNamespace>() -> bool {
    conversion_bytes::<W>() <= 65536
        && retained_bytes::<W>() + conversion_bytes::<W>() <= 196608
        && super::super::super::budget::fits::<W>()
        && super::fits::<W>()
}

#[cfg(test)]
#[path = "empty_transactions_tests.rs"]
mod tests;

#[path = "empty_transactions_live.rs"]
mod live;
pub(in crate::storage) use live::EmptyTransactionInstall;
pub(crate) use live::EmptyTransactionObservation;

pub(in super::super) use live::{EmptyLoadInstall, History};
