//! Short source reservations in the original isolation ledger. Full and resident
//! Catalog lookup and the Catalog Commit/retirement/lifecycle writers participate;
//! unconverted writer/reader entries are not covered by this aggregate bound.
use super::*;
use crate::storage::session_fence::SessionFence;
use crate::transaction::concurrency::{TerminalReadGate, terminal_gate_source_bytes};
use std::{mem::size_of, sync::atomic::Ordering};

const SHORT_BYTES: u64 = 65_536;
const USED: std::ops::Range<usize> = 24..32;

#[derive(Clone, Copy)]
pub(super) enum OperationClass {
    CatalogFull,
    CatalogResident,
    /// Terminal progress. Its charge changes only at the read-gate and Drain
    /// boundaries, never while an admitted reader could hold a loan.
    CommitProgress,
    CatalogProgress,
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

/// Fixed Catalog Commit charges, computed once during split like the reader
/// charges. stage runs with new reads closed and admitted reads returned;
/// pending may overlap readers; install starts after the original Drain;
/// publish follows install.finish, where reads may overlap again.
pub(super) struct CatalogCommitCharges {
    shared: usize,
    stage: usize,
    pending: usize,
    install: usize,
    publish: usize,
}
impl CatalogCommitCharges {
    #[cfg(test)]
    pub(super) fn observations(&self) -> [usize; 5] {
        [
            self.shared,
            self.stage,
            self.pending,
            self.install,
            self.publish,
        ]
    }
    pub(super) fn new<W: WriterNamespace>() -> Self {
        let [stage, pending, install, publish] = create_pair::catalog_commit_writer_windows();
        let carriers = commit_carrier_bytes() + commit_input_bytes();
        Self {
            shared: shared_bytes::<W>(),
            stage: stage.max(create_pair::catalog_live::commit_prepare_source_bytes::<W>())
                + carriers,
            pending: pending + carriers,
            install: install + carriers,
            publish: publish + carriers,
        }
    }
}

/// The largest writer window, for the session's single-operation census.
pub(super) fn commit_source_peak<W: WriterNamespace>() -> usize {
    let charges = CatalogCommitCharges::new::<W>();
    charges
        .stage
        .max(charges.pending)
        .max(charges.install)
        .max(charges.publish)
}

/// Unlike the former Prepared-consuming outer entry, the synchronous entry
/// retains its original actor argument and private loan while the continuation
/// waits. No last-use or optimizer-based discount is taken in any window.
fn commit_input_bytes() -> usize {
    size_of::<crate::storage::transaction_record::TxnRef>()
        + size_of::<&mut [u8; crate::storage::semantic_catalog::PRIVATE_BYTES]>()
}

/// Retirement application retains one fixed progress charge through
/// publication and checked cleanup. Its physical closures are consecutive.
pub(super) fn retirement_source_peak() -> usize {
    create_pair::catalog_live::retirement::source_bytes()
        + commit_carrier_bytes()
        + 2 * size_of::<Result<crate::storage::semantic_catalog::Observation>>()
        + 2 * size_of::<crate::storage::session_fence::UnwindFence<'_>>()
        + retirement_entry_bytes()
}

/// A fixed split-time cost. Operation callers cannot select a byte count.
pub(super) struct CatalogProgressCharge(usize);
impl CatalogProgressCharge {
    pub(super) fn new<W: WriterNamespace>() -> Self {
        let bodies = create_pair::catalog_live::catalog_live_source_bytes::<W>();
        Self(bodies.retirement.max(bodies.lifecycle))
    }
}

/// The synchronous writer keeps the admission and gate through preparation,
/// application and result cleanup. Prepared is constructed and consumed inside
/// that call; no preparation receipt escapes to an uncharged caller.
pub(super) fn retirement_prepare_carrier_bytes() -> usize {
    commit_carrier_bytes()
        + 3 * size_of::<Result<PreparedCatalog<'_, '_, '_>>>()
        + size_of::<crate::storage::session_fence::UnwindFence<'_>>()
        + 2 * size_of::<Result<crate::storage::semantic_catalog::Observation>>()
        + retirement_entry_bytes()
}

/// Six finite lifecycle actions share one private dispatch. Three typed
/// Prepared results cover common preparation, action adapter and match output;
/// Observation results cover application forwarding and checked cleanup.
pub(super) fn lifecycle_carrier_bytes<W: WriterNamespace>() -> usize {
    commit_carrier_bytes()
        + 3 * size_of::<Result<PreparedCatalog<'_, '_, '_>>>()
        + 2 * size_of::<crate::storage::session_fence::UnwindFence<'_>>()
        + 3 * size_of::<Result<crate::storage::semantic_catalog::Observation>>()
        + create_pair::catalog_live::lifecycle::entry_carrier_bytes::<W>()
}

/// The unified caller's ordinal/operation and borrowed arguments survive into
/// either closure. Count both by-value arguments and caller packet copies.
fn retirement_entry_bytes() -> usize {
    2 * size_of::<crate::storage::semantic_catalog::Operation>()
        + 2 * size_of::<usize>()
        // Synchronous inputs self/owner/private/flusher (4); immediate
        // closure captures those plus ordinal/operation (6); exclusive
        // DiskSession caller self/owner/private (3). Keep this packet distinct
        // from commit_carrier's original application and cleanup references.
        + (4 + 6 + 3) * size_of::<&()>()
}

/// The retained admission and gate, their Results and caller conversions, and
/// one ledger transition/release or gate reopen closure, including the core
/// state validation the reopen performs. Charged in every window. The gate's
/// close/wait precedes reservation like a reader's entry.
fn commit_carrier_bytes() -> usize {
    size_of::<OperationSourceAdmission<'_, '_, '_, '_>>()
        + 2 * size_of::<SourceResult<OperationSourceAdmission<'_, '_, '_, '_>>>()
        + size_of::<Option<TerminalReadGate<'_, '_>>>()
        + 2 * size_of::<SourceResult<()>>()
        + 2 * size_of::<SourceFailure>()
        + 2 * size_of::<RetentionError>()
        + 4 * size_of::<&()>()
        + leaf_source_bytes().max(terminal_gate_source_bytes())
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
            // Only the dedicated writer entries, holding a read gate, admit a writer;
            // reserve_class rejects the zero charge as a fenced Invariant.
            OperationClass::CommitProgress | OperationClass::CatalogProgress => 0,
        };
        Self::reserve_class(ledger, charges.shared, charge, class)
    }

    /// Reserve the stage window after the gate has closed new reads and every
    /// admitted read returned its loan. No wait: insufficient space can only
    /// come from an unconverted holder and stays Unchanged Capacity.
    pub(super) fn catalog_commit(
        ledger: &'b Mutex<IsolationLedger<'a, 'c, 's>>,
        charges: &CatalogCommitCharges,
        gate: &TerminalReadGate<'_, '_>,
    ) -> SourceResult<Self> {
        {
            let state = ledger
                .lock()
                .map_err(|poison| failed(poison.into_inner().fence, SourceCause::Poisoned))?;
            if !gate.belongs_to(state.core) {
                return Err(failed(state.fence, SourceCause::Invariant));
            }
        }
        // Window order is what makes every later boundary shrink-only or a
        // no-wait growth under Drain. A violated census is refused before any
        // effect instead of fencing after the WAL stage.
        let ordered = charges.pending <= charges.stage
            && charges.publish <= charges.install
            && [charges.stage, charges.install]
                .iter()
                .all(|charge| charges.shared.saturating_add(*charge) <= SHORT_BYTES as usize);
        if !ordered {
            return Err(capacity());
        }
        Self::reserve_class(
            ledger,
            charges.shared,
            charges.stage,
            OperationClass::CommitProgress,
        )
    }

    /// The caller holds the original closed read gate throughout fixed progress.
    /// The fixed charge was computed during exclusive split, before any entry.
    pub(super) fn catalog_progress(
        ledger: &'b Mutex<IsolationLedger<'a, 'c, 's>>,
        charges: &CatalogCommitCharges,
        progress: &CatalogProgressCharge,
        gate: &TerminalReadGate<'_, '_>,
    ) -> SourceResult<Self> {
        {
            let state = ledger
                .lock()
                .map_err(|poison| failed(poison.into_inner().fence, SourceCause::Poisoned))?;
            if !gate.belongs_to(state.core) {
                return Err(failed(state.fence, SourceCause::Invariant));
            }
        }
        Self::reserve_class(
            ledger,
            charges.shared,
            progress.0,
            OperationClass::CatalogProgress,
        )
    }

    /// After stage returned Pending and before reads reopen. Shrink only.
    pub(super) fn enter_commit_pending(
        &mut self,
        charges: &CatalogCommitCharges,
    ) -> SourceResult<()> {
        self.transition(charges.shared, charges.stage, charges.pending, false)
    }

    /// After the original Drain: no admitted reader holds a loan and new reads
    /// are rejected, so growth never waits. Exceeding the limit is a broken
    /// accounting invariant, not Capacity.
    pub(super) fn enter_commit_install<const N: usize>(
        &mut self,
        charges: &CatalogCommitCharges,
        _drained: &dirty_frame::DrainedCommitTargets<'_, '_, '_, '_, '_, '_, N>,
    ) -> SourceResult<()> {
        self.transition(charges.shared, charges.pending, charges.install, true)
    }

    /// After install.finish reopened reads, before the Q publication. Shrink only.
    pub(super) fn enter_commit_publish(
        &mut self,
        charges: &CatalogCommitCharges,
    ) -> SourceResult<()> {
        self.transition(charges.shared, charges.install, charges.publish, false)
    }

    fn transition(
        &mut self,
        shared: usize,
        from: usize,
        to: usize,
        may_grow: bool,
    ) -> SourceResult<()> {
        let mut state = self
            .ledger
            .lock()
            .map_err(|poison| failed(poison.into_inner().fence, SourceCause::Poisoned))?;
        let used = check(&state)?;
        if !self.active
            || !matches!(self.class, OperationClass::CommitProgress)
            || u64::from(self.charge) != from as u64
            || to == 0
            || (!may_grow && to > from)
        {
            return Err(failed(state.fence, SourceCause::Invariant));
        }
        let next = SHORT_BYTES
            .checked_sub(shared as u64)
            .and_then(|available| {
                used.checked_sub(from as u64)
                    .and_then(|rest| rest.checked_add(to as u64))
                    .filter(|next| *next <= available)
            })
            .ok_or_else(|| failed(state.fence, SourceCause::Invariant))?;
        state.bytes[USED].copy_from_slice(&next.to_le_bytes());
        // checked next <= available <= SHORT_BYTES proves this conversion.
        self.charge = to as u32;
        Ok(())
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
            OperationClass::CatalogFull
            | OperationClass::CatalogResident
            | OperationClass::CommitProgress
            | OperationClass::CatalogProgress
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
    ) + crate::transaction::concurrency::core_source_bytes()
        + split_port_bytes::<W>()
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
    let compute = SPLIT_CALCULATION_BYTES
        + 3 * size_of::<CatalogSourceCharges>()
        + 3 * size_of::<CatalogCommitCharges>()
        + 3 * size_of::<CatalogProgressCharge>();
    let returning = 2 * split_port_bytes::<W>()
        + size_of::<CatalogSourceCharges>()
        + size_of::<CatalogCommitCharges>()
        + size_of::<CatalogProgressCharge>();
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
    pub(in crate::storage) fn catalog_test_fixed_charges(&self) -> [usize; 3] {
        self.source_charges.observations()
    }

    pub(in crate::storage) fn catalog_test_source_used(&self) -> u64 {
        test_source_used(self.isolation)
    }

    /// Hold one plain loan in the original ledger while `run` executes.
    pub(in crate::storage) fn catalog_test_with_held_source<T>(
        &self,
        bytes: usize,
        run: impl FnOnce() -> T,
    ) -> T {
        let loan =
            OperationSourceAdmission::reserve(self.isolation, self.source_charges.shared, bytes)
                .unwrap();
        let result = run();
        drop(loan);
        result
    }
}

#[cfg(test)]
fn test_source_used(isolation: &Mutex<IsolationLedger<'_, '_, '_>>) -> u64 {
    let state = isolation
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    u64::from_le_bytes(state.bytes[USED].try_into().unwrap())
}

#[cfg(test)]
impl<W: WriterNamespace> MutationPort<'_, '_, '_, '_, '_, '_, '_, '_, '_, W> {
    pub(in crate::storage) fn catalog_test_retirement_charge(&self) -> usize {
        self.progress_charge.0
    }

    pub(in crate::storage) fn catalog_test_commit_charges(&self) -> [usize; 5] {
        self.commit_charges.observations()
    }

    pub(in crate::storage) fn catalog_test_source_used(&self) -> u64 {
        test_source_used(self.isolation)
    }
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
