//! Short source reservations in the original isolation ledger. Full and resident
//! Catalog lookup currently participate; unconverted writer/reader entries
//! are not covered by this partial supply's aggregate bound.
use super::*;
use crate::storage::session_fence::SessionFence;
use std::{mem::size_of, sync::atomic::Ordering};

const SHORT_BYTES: u64 = 65_536;
const USED: std::ops::Range<usize> = 24..32;

#[derive(Clone, Copy)]
pub(super) enum OperationClass {
    CatalogFull,
    CatalogResident,
}

/// Fixed type-derived costs, computed once while DiskSession is exclusively
/// borrowed for split. ReadPort owns these immutable values; callers cannot
/// supply a byte count or recompute a recursive census before each admission.
pub(super) struct CatalogSourceCharges {
    shared: usize,
    full: usize,
    resident: usize,
}
impl CatalogSourceCharges {
    #[cfg(test)]
    pub(super) fn observations(&self) -> [usize; 3] {
        [self.shared, self.full, self.resident]
    }
    pub(super) fn new<W: WriterNamespace>() -> Self {
        Self {
            shared: shared_bytes::<W>(),
            full: read::lookup_source_bytes::<W>(),
            resident: read::resident_lookup_source_bytes::<W>(),
        }
    }
}

/// A non-Clone accounting loan, never a page, snapshot or WAL authority.
pub(super) struct OperationSourceAdmission<'b, 'a, 'c, 's> {
    ledger: &'b Mutex<IsolationLedger<'a, 'c, 's>>,
    class: OperationClass,
    charge: u32,
    active: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum SourceCause {
    Capacity,
    Fenced,
    Poisoned,
    BindingMismatch,
    Invariant,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct SourceFailure {
    pub(super) cause: SourceCause,
    pub(super) effect: Effect,
}
type SourceResult<T> = std::result::Result<T, SourceFailure>;
impl From<SourceFailure> for RetentionError {
    fn from(error: SourceFailure) -> Self {
        Self {
            phase: RetentionPhase::Census,
            cause: match error.cause {
                SourceCause::Capacity => RetentionCause::Capacity,
                SourceCause::Fenced => RetentionCause::Fenced,
                SourceCause::Poisoned => RetentionCause::Poisoned,
                SourceCause::BindingMismatch => RetentionCause::BindingMismatch,
                SourceCause::Invariant => RetentionCause::Invariant,
            },
            effect: error.effect,
        }
    }
}
fn failed(fence: &SessionFence, cause: SourceCause) -> SourceFailure {
    fence.fence();
    SourceFailure {
        cause,
        effect: Effect::CallerMustResolve,
    }
}
fn capacity() -> SourceFailure {
    SourceFailure {
        cause: SourceCause::Capacity,
        effect: Effect::Unchanged,
    }
}

fn check(state: &IsolationLedger<'_, '_, '_>) -> SourceResult<u64> {
    if std::thread::panicking() {
        state.fence.fence();
    }
    if state.fence.is_fenced() {
        return Err(failed(state.fence, SourceCause::Fenced));
    }
    if !state.core.uses_fence(state.fence)
        || state.binding.load(Ordering::Acquire) != super::super::super::BOUND
    {
        return Err(failed(state.fence, SourceCause::BindingMismatch));
    }
    let used = u64::from_le_bytes(state.bytes[USED].try_into().expect("fixed source field"));
    if used > SHORT_BYTES || state.bytes[32..64].iter().any(|byte| *byte != 0) {
        return Err(failed(state.fence, SourceCause::Invariant));
    }
    Ok(used)
}

impl<'b, 'a, 'c, 's> OperationSourceAdmission<'b, 'a, 'c, 's> {
    /// Charge the fixed lookup closure inside its original progress lifetime,
    /// before refs or pins. The caller returns progress if reservation fails.
    /// No caller chooses a byte count; the charge cannot grow during lookup.
    pub(super) fn catalog(
        ledger: &'b Mutex<IsolationLedger<'a, 'c, 's>>,
        charges: &CatalogSourceCharges,
        class: OperationClass,
    ) -> SourceResult<Self> {
        let charge = match class {
            OperationClass::CatalogFull => charges.full,
            OperationClass::CatalogResident => charges.resident,
        };
        Self::reserve_class(ledger, charges.shared, charge, class)
    }

    pub(super) fn resident(&self) -> bool {
        matches!(self.class, OperationClass::CatalogResident)
    }

    #[cfg(test)]
    fn reserve(
        ledger: &'b Mutex<IsolationLedger<'a, 'c, 's>>,
        shared: usize,
        charge: usize,
    ) -> SourceResult<Self> {
        Self::reserve_class(ledger, shared, charge, OperationClass::CatalogFull)
    }

    fn reserve_class(
        ledger: &'b Mutex<IsolationLedger<'a, 'c, 's>>,
        shared: usize,
        charge: usize,
        class: OperationClass,
    ) -> SourceResult<Self> {
        let mut state = ledger
            .lock()
            .map_err(|poison| failed(poison.into_inner().fence, SourceCause::Poisoned))?;
        let used = check(&state)?;
        let available = SHORT_BYTES
            .checked_sub(shared as u64)
            .ok_or_else(capacity)?;
        if charge == 0 || used > available {
            return Err(failed(state.fence, SourceCause::Invariant));
        }
        let next = used
            .checked_add(charge as u64)
            .filter(|next| *next <= available)
            .ok_or_else(capacity)?;
        state.bytes[USED].copy_from_slice(&next.to_le_bytes());
        Ok(Self {
            ledger,
            class,
            // checked next <= available <= SHORT_BYTES proves this conversion.
            charge: charge as u32,
            active: true,
        })
    }

    /// Consume the carrier before the original progress ticket is returned.
    /// Failure keeps active set, so Drop retries without inventing capacity.
    pub(super) fn finish_checked(mut self) -> SourceResult<()> {
        self.release_checked()
    }

    /// Called after output decoding/Result construction as well as reference
    /// cleanup. Fatal bookkeeping remains visible, matching isolation credit
    /// cleanup; no poison reset or manufactured capacity is allowed.
    pub(super) fn release_checked(&mut self) -> SourceResult<()> {
        if !self.active {
            return Ok(());
        }
        let mut state = self
            .ledger
            .lock()
            .map_err(|poison| failed(poison.into_inner().fence, SourceCause::Poisoned))?;
        let used = check(&state)?;
        match self.class {
            OperationClass::CatalogFull | OperationClass::CatalogResident
                if self.charge != 0 && u64::from(self.charge) <= SHORT_BYTES => {}
            _ => return Err(failed(state.fence, SourceCause::Invariant)),
        }
        let next = used
            .checked_sub(u64::from(self.charge))
            .ok_or_else(|| failed(state.fence, SourceCause::Invariant))?;
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
    ) + split_port_bytes::<W>()
}

/// The single non-Clone split tuple is consumed into its three ports. Their
/// actual values remain beside the owner once, including ReadPort's costs.
pub(super) fn split_port_bytes<W: WriterNamespace>() -> usize {
    size_of::<MutationPort<'_, '_, '_, '_, '_, '_, '_, '_, '_, W>>()
        + size_of::<ReadPort<'_, '_, '_, '_, '_, '_, '_, '_, W>>()
        + size_of::<MaintenancePort<'_, '_, '_, '_, '_, '_, '_, W>>()
}

/// A deliberately loose bound for evaluating the fixed census expression graph
/// during exclusive split: at most 24 nested arithmetic helpers, each with at
/// most 128 usize-sized argument/local/intermediate/iterator/return values.
/// The graph has no recursion, I/O, authorities, page arrays or decoded values:
/// size_of<T> returns a usize without constructing T. Both complete returned
/// census packets and the caller's extracted packet fit the per-helper bound.
/// See the split-charge fixture's owner-reviewed graph inventory. This is a
/// source-value upper bound, not an assumed compiler constant-folding/stack size.
pub(super) const SPLIT_CALCULATION_BYTES: usize = 24 * 128 * size_of::<usize>();

/// The charge computation completes before any port is constructed. In the
/// return phase the tuple and all extracted ports are conservatively counted.
pub(super) fn split_source_bytes<W: WriterNamespace>() -> usize {
    let compute = SPLIT_CALCULATION_BYTES + 3 * size_of::<CatalogSourceCharges>();
    let returning = 2 * split_port_bytes::<W>() + size_of::<CatalogSourceCharges>();
    4 * size_of::<&()>() + compute.max(returning)
}

pub(super) fn leaf_source_bytes() -> usize {
    // Reservation/release, lock success/poison, checked arithmetic, and return
    // carriers. The ledger backing is shared retained metadata, counted once.
    2 * size_of::<OperationSourceAdmission<'_, '_, '_, '_>>()
        + 2 * size_of::<SourceResult<OperationSourceAdmission<'_, '_, '_, '_>>>()
        + 4 * size_of::<std::sync::MutexGuard<'_, IsolationLedger<'_, '_, '_>>>()
        + 4 * size_of::<SourceResult<()>>()
        + 4 * size_of::<SourceFailure>()
        + 16 * size_of::<usize>()
        + 8 * size_of::<&()>()
}

/// Observation only, in waiting/guarded/guardless-return order. The original
/// isolation guard serializes just the middle column; no new owner is created.
#[cfg(test)]
pub(super) fn entry_source_observation() -> (usize, usize, usize) {
    type Guard<'a> = std::sync::MutexGuard<'a, IsolationLedger<'a, 'a, 'a>>;
    // catalog's three inputs + selected charge, then reserve_class's inputs.
    let waiting = 3 * size_of::<&()>() + 3 * size_of::<usize>() + 2 * size_of::<OperationClass>();
    let guarded = 4 * size_of::<Guard<'_>>()
        + 2 * size_of::<std::sync::LockResult<Guard<'_>>>()
        + 2 * size_of::<std::sync::PoisonError<Guard<'_>>>()
        + 4 * size_of::<SourceResult<()>>()
        + 4 * size_of::<SourceResult<u64>>()
        + 4 * size_of::<SourceFailure>()
        + 4 * size_of::<Option<u64>>()
        + 4 * size_of::<[u8; 8]>()
        + 6 * size_of::<u64>()
        + 8 * size_of::<&()>()
        + 2 * size_of::<&[u8]>()
        + 2 * size_of::<std::slice::Iter<'_, u8>>()
        + 8 * size_of::<bool>();
    let returning = 2 * size_of::<OperationSourceAdmission<'_, '_, '_, '_>>()
        + 2 * size_of::<SourceResult<OperationSourceAdmission<'_, '_, '_, '_>>>()
        + 4 * size_of::<SourceFailure>()
        + 4 * size_of::<&()>()
        + 4 * size_of::<usize>();
    (waiting, guarded, returning)
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
