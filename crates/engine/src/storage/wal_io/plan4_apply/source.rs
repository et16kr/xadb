//! Short source reservations in the original isolation ledger. Only the full
//! Catalog lookup currently participates; unconverted writer/reader entries
//! are not covered by this partial supply's aggregate bound.
use super::*;
use crate::storage::session_fence::SessionFence;
use std::{mem::size_of, sync::atomic::Ordering};

const SHORT_BYTES: u64 = 65_536;
const USED: std::ops::Range<usize> = 24..32;

enum OperationClass {
    CatalogFull,
}

/// A non-Clone accounting loan, never a page, snapshot or WAL authority.
pub(super) struct OperationSourceAdmission<'b, 'a, 'c, 's> {
    ledger: &'b Mutex<IsolationLedger<'a, 'c, 's>>,
    class: OperationClass,
    charge: u64,
    active: bool,
}

fn failed(fence: &SessionFence, cause: RetentionCause) -> RetentionError {
    fence.fence();
    RetentionError {
        phase: RetentionPhase::Census,
        cause,
        effect: Effect::CallerMustResolve,
    }
}

fn check(state: &IsolationLedger<'_, '_, '_>) -> Result<u64> {
    if std::thread::panicking() {
        state.fence.fence();
    }
    if state.fence.is_fenced() {
        return Err(failed(state.fence, RetentionCause::Fenced));
    }
    if !state.core.uses_fence(state.fence)
        || state.binding.load(Ordering::Acquire) != super::super::super::BOUND
    {
        return Err(failed(state.fence, RetentionCause::BindingMismatch));
    }
    let used = u64::from_le_bytes(state.bytes[USED].try_into().expect("fixed source field"));
    if used > SHORT_BYTES || state.bytes[32..64].iter().any(|byte| *byte != 0) {
        return Err(failed(state.fence, RetentionCause::Invariant));
    }
    Ok(used)
}

impl<'b, 'a, 'c, 's> OperationSourceAdmission<'b, 'a, 'c, 's> {
    /// Charge the fixed full lookup closure after progress entry, before refs or pins.
    /// No caller chooses a byte count; the charge cannot grow during lookup.
    pub(super) fn catalog<W: WriterNamespace>(
        ledger: &'b Mutex<IsolationLedger<'a, 'c, 's>>,
    ) -> Result<Self> {
        Self::reserve(
            ledger,
            shared_bytes::<W>(),
            read::lookup_source_bytes::<W>(),
        )
    }

    fn reserve(
        ledger: &'b Mutex<IsolationLedger<'a, 'c, 's>>,
        shared: usize,
        charge: usize,
    ) -> Result<Self> {
        let mut state = ledger
            .lock()
            .map_err(|poison| failed(poison.into_inner().fence, RetentionCause::Poisoned))?;
        let used = check(&state)?;
        let available = SHORT_BYTES
            .checked_sub(shared as u64)
            .ok_or_else(|| unchanged(RetentionPhase::Census, RetentionCause::Capacity))?;
        if charge == 0 || used > available {
            return Err(failed(state.fence, RetentionCause::Invariant));
        }
        let next = used
            .checked_add(charge as u64)
            .filter(|next| *next <= available)
            .ok_or_else(|| unchanged(RetentionPhase::Census, RetentionCause::Capacity))?;
        state.bytes[USED].copy_from_slice(&next.to_le_bytes());
        Ok(Self {
            ledger,
            class: OperationClass::CatalogFull,
            charge: charge as u64,
            active: true,
        })
    }

    /// Called after output decoding/Result construction as well as reference
    /// cleanup. Fatal bookkeeping remains visible, matching isolation credit
    /// cleanup; no poison reset or manufactured capacity is allowed.
    pub(super) fn release_checked(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }
        let mut state = self
            .ledger
            .lock()
            .map_err(|poison| failed(poison.into_inner().fence, RetentionCause::Poisoned))?;
        let used = check(&state)?;
        match self.class {
            OperationClass::CatalogFull if self.charge != 0 && self.charge <= SHORT_BYTES => {}
            _ => return Err(failed(state.fence, RetentionCause::Invariant)),
        }
        let next = used
            .checked_sub(self.charge)
            .ok_or_else(|| failed(state.fence, RetentionCause::Invariant))?;
        state.bytes[USED].copy_from_slice(&next.to_le_bytes());
        self.active = false;
        Ok(())
    }
}

impl Drop for OperationSourceAdmission<'_, '_, '_, '_> {
    fn drop(&mut self) {
        let _ = self.release_checked();
    }
}

/// One retained owner, using the larger fresh/recovered source layout. The
/// recovered runtime already contains its MutableStorageOwner; do not add it
/// again. This is the same identity used by both existing source censuses.
pub(super) fn shared_bytes<W: WriterNamespace>() -> usize {
    use super::super::super::plan4_recovery::Plan4RecoveryRuntime;
    (size_of::<MutableStorageOwner<'_, '_, W>>()
        + size_of::<DiskSession<'_, '_, '_, '_, '_, '_, '_, '_, W>>())
    .max(
        size_of::<Plan4RecoveryRuntime<'_, '_, '_, W>>()
            + size_of::<Plan4Recovered<'_, '_, '_, '_, '_, '_, '_, '_, W>>(),
    ) + crate::transaction::concurrency::core_source_bytes()
}

pub(super) fn leaf_source_bytes() -> usize {
    // Reservation/release, lock success/poison, checked arithmetic, and return
    // carriers. The ledger backing is shared retained metadata, counted once.
    2 * size_of::<OperationSourceAdmission<'_, '_, '_, '_>>()
        + 2 * size_of::<Result<OperationSourceAdmission<'_, '_, '_, '_>>>()
        + 4 * size_of::<std::sync::MutexGuard<'_, IsolationLedger<'_, '_, '_>>>()
        + 4 * size_of::<Result<()>>()
        + 4 * size_of::<RetentionError>()
        + 16 * size_of::<usize>()
        + 8 * size_of::<&()>()
}

#[cfg(test)]
impl<W: WriterNamespace> ReadPort<'_, '_, '_, '_, '_, '_, '_, '_, W> {
    pub(in crate::storage) fn catalog_test_source_used(&self) -> u64 {
        let state = self
            .isolation
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        u64::from_le_bytes(state.bytes[USED].try_into().unwrap())
    }
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
