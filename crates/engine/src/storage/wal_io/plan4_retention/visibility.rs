//! Bounded volatile visibility bookkeeping for the original plan4 owner.
//! The actual source-validation, Commit and source-latch paths issue sealed inputs.
//! No decoder, raw sequence or reference can create one of those capabilities.
#![allow(dead_code)] // Connected by the same CREATE consumer change, not a second owner.

use crate::{
    storage::{
        session_fence::SessionFence, transaction_record::TxnRef, undo_ref::UndoRef,
        wal_io::WalRequirement,
    },
    transaction::concurrency::{
        ConcurrencyError, Owner, OwnerIdentity, PageConcurrency, VersionEditGuard, VersionIdentity,
    },
};
use foundation::error::Effect;
use std::sync::{
    Mutex, MutexGuard,
    atomic::{AtomicU8, Ordering},
};

#[path = "visibility/binding.rs"]
mod binding;
pub(super) use binding::source_bytes as binding_source_bytes;

#[path = "visibility/epochs.rs"]
mod epochs;
pub(super) use epochs::{
    change_carriers as epoch_change_carriers, commit_carriers as epoch_commit_carriers,
    transition_source_bytes as epoch_transition_source_bytes,
};

#[cfg(test)]
pub(super) use epochs::commit_retained_carrier as epoch_commit_retained_carrier;

#[path = "visibility/authority.rs"]
mod authority;
#[path = "visibility/catalog_retirement.rs"]
mod catalog_retirement;
#[path = "visibility/pins.rs"]
mod pins;
#[path = "visibility/retirement.rs"]
mod retirement;
#[path = "visibility/snapshot.rs"]
mod snapshot;
#[path = "visibility/source.rs"]
mod source;

pub(in crate::storage) use authority::{
    AppliedRetirement, CatalogSource, InstalledFrontier, RetirementScope, RollbackSource,
    SourceFrontier, StagedRetirement, VerifiedRetirement,
};
pub(in crate::storage) use pins::{CapturedCatalogRefs, ReferenceLease, TxnPin, UndoPin};
pub(in crate::storage) use retirement::{RetirementIntent, RetirementReject};
pub(in crate::storage) use snapshot::{SnapshotGuard, SnapshotReservation};

pub(in crate::storage) const VISIBILITY_BYTES: usize = 8192;
pub(in crate::storage) const EPOCH_BYTES: usize = 1600;
const SNAPSHOTS: usize = 68;
const USERS: usize = 32;
const REFERENCES: usize = 66;
const SNAPSHOT_START: usize = 96;
const SNAPSHOT_BYTES: usize = 32;
const ADMISSION_START: usize = SNAPSHOT_START + SNAPSHOTS * SNAPSHOT_BYTES;
const ADMISSION_BYTES: usize = 16;
const REFERENCE_START: usize = ADMISSION_START + USERS * ADMISSION_BYTES;
const REFERENCE_BYTES: usize = 80;
const RETIREMENT_START: usize = REFERENCE_START + REFERENCES * REFERENCE_BYTES;
const RETIREMENT_BYTES: usize = 128;
const _: () = assert!(RETIREMENT_START + RETIREMENT_BYTES == VISIBILITY_BYTES);
const MAGIC: &[u8; 8] = b"XVISRAM1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::storage) enum VisibilityCause {
    Fenced,
    Poisoned,
    BindingMismatch,
    Capacity,
    Exhausted,
    Busy,
    Invariant,
    Concurrency(crate::transaction::concurrency::Cause),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::storage) struct VisibilityError {
    pub(in crate::storage) cause: VisibilityCause,
    pub(in crate::storage) effect: Effect,
}
pub(in crate::storage) type VisibilityResult<T> = Result<T, VisibilityError>;
impl From<ConcurrencyError> for VisibilityError {
    fn from(error: ConcurrencyError) -> Self {
        Self {
            cause: VisibilityCause::Concurrency(error.cause),
            effect: error.effect,
        }
    }
}
fn unchanged(cause: VisibilityCause) -> VisibilityError {
    VisibilityError {
        cause,
        effect: Effect::Unchanged,
    }
}

/// Identity comparisons do not issue an operation, lease or storage binding.
#[derive(Clone, Copy)]
struct Origin<'c, 's> {
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    fence: &'s SessionFence,
}
impl Origin<'_, '_> {
    fn same(&self, other: &Self) -> bool {
        std::ptr::eq(self.core, other.core)
            && std::ptr::eq(self.binding, other.binding)
            && std::ptr::eq(self.fence, other.fence)
    }
    fn health(&self) -> VisibilityResult<()> {
        if self.fence.is_fenced() {
            return Err(self.fatal(VisibilityCause::Fenced));
        }
        if !self.core.uses_fence(self.fence)
            || self.binding.load(Ordering::Acquire) != super::super::super::BOUND
        {
            return Err(self.fatal(VisibilityCause::BindingMismatch));
        }
        Ok(())
    }
    fn fatal(&self, cause: VisibilityCause) -> VisibilityError {
        self.fence.fence();
        VisibilityError {
            cause,
            effect: Effect::CallerMustResolve,
        }
    }
}

/// A disjoint part of the original metadata reservation. Construction of this
/// storage-only loan neither initializes Q nor binds it to a session.
pub(in crate::storage) struct VisibilityLoan<'a> {
    pub(in crate::storage) bytes: &'a mut [u8; VISIBILITY_BYTES],
    pub(in crate::storage) epochs: &'a mut [u8; EPOCH_BYTES],
}

/// The original publication mutex owns both Q and the future table epoch lane.
/// The activation bit stays closed through recovery cleanup and source errors.
/// Published table identities are never evicted in this open.
struct Publication<'a> {
    q: u64,
    epochs_active: bool,
    epochs: &'a mut [u8; EPOCH_BYTES],
}

pub(in crate::storage) struct BoundVisibility<'a, 'c, 's> {
    origin: Origin<'c, 's>,
    published: Mutex<Publication<'a>>,
    registry: Mutex<&'a mut [u8; VISIBILITY_BYTES]>,
}

/// Not Clone/Copy: the original retirement target wrapper keeps this observation
/// with the exact version hold. The generation also rejects a later gate hold.
pub(in crate::storage) struct HorizonObservation<'c, 's> {
    origin: Origin<'c, 's>,
    version: VersionIdentity,
    published: u64,
    horizon: u64,
}
impl HorizonObservation<'_, '_> {
    pub(in crate::storage) fn published(&self) -> u64 {
        self.published
    }
    pub(in crate::storage) fn horizon(&self) -> u64 {
        self.horizon
    }
}

impl<'a, 'c, 's> BoundVisibility<'a, 'c, 's> {
    fn same_registry(&self, other: &BoundVisibility<'_, '_, '_>) -> bool {
        std::ptr::eq(
            std::ptr::from_ref(self).cast::<()>(),
            std::ptr::from_ref(other).cast::<()>(),
        )
    }

    pub(super) fn bind(
        loan: VisibilityLoan<'a>,
        ready: SourceFrontier<'c, 's>,
    ) -> VisibilityResult<Self> {
        ready.origin.health()?;
        loan.bytes.fill(0);
        if !ready.epochs_active {
            loan.epochs.fill(0);
        }
        loan.bytes[..8].copy_from_slice(MAGIC);
        Ok(Self {
            origin: ready.origin,
            published: Mutex::new(Publication {
                q: ready.q,
                epochs_active: ready.epochs_active,
                epochs: loan.epochs,
            }),
            registry: Mutex::new(loan.bytes),
        })
    }
    pub(in crate::storage) fn matches_core(&self, core: &PageConcurrency<'_>) -> bool {
        std::ptr::eq(self.origin.core, core)
    }
    fn entry(&self) -> VisibilityResult<MutexGuard<'_, &'a mut [u8; VISIBILITY_BYTES]>> {
        self.origin.health()?;
        let bytes = self
            .registry
            .lock()
            .map_err(|_| self.origin.fatal(VisibilityCause::Poisoned))?;
        self.origin.health()?;
        validate(&bytes).map_err(|_| self.origin.fatal(VisibilityCause::Invariant))?;
        Ok(bytes)
    }
    fn publication(&self) -> VisibilityResult<MutexGuard<'_, Publication<'a>>> {
        self.origin.health()?;
        let q = self
            .published
            .lock()
            .map_err(|_| self.origin.fatal(VisibilityCause::Poisoned))?;
        self.origin.health()?;
        Ok(q)
    }
    /// Registry cleanup never returns a poisoned/fenced reservation for reuse.
    fn cleanup(&self) -> VisibilityResult<MutexGuard<'_, &'a mut [u8; VISIBILITY_BYTES]>> {
        if std::thread::panicking() {
            self.origin.fence.fence();
        }
        self.entry()
    }
    pub(in crate::storage) fn capture_horizon(
        &self,
        version: &VersionEditGuard<'_, 'c, 's>,
    ) -> VisibilityResult<HorizonObservation<'c, 's>> {
        let identity = version.visibility_identity(self.origin.core)?;
        let published = self.publication()?.q;
        // The temporary publication guard is gone before entering the registry.
        let bytes = self.entry()?;
        let mut horizon = published;
        for index in 0..SNAPSHOTS {
            let row = snapshot_row(&bytes[..], index);
            if row[0] == 2 {
                let q = u64_at(row, 16);
                if q > published {
                    return Err(self.origin.fatal(VisibilityCause::Invariant));
                }
                horizon = horizon.min(q);
            }
        }
        Ok(HorizonObservation {
            origin: self.origin,
            version: identity,
            published,
            horizon,
        })
    }
    fn check_horizon(
        &self,
        observation: &HorizonObservation<'c, 's>,
        version: &VersionEditGuard<'_, 'c, 's>,
    ) -> VisibilityResult<()> {
        if !self.origin.same(&observation.origin) {
            return Err(unchanged(VisibilityCause::BindingMismatch));
        }
        if version.visibility_identity(self.origin.core)? != observation.version {
            return Err(unchanged(VisibilityCause::BindingMismatch));
        }
        Ok(())
    }
    /// The original target wrapper calls this before returning its version
    /// hold. A forgotten intent must not silently release its serialization.
    pub(in crate::storage) fn check_retirement_released(&self) -> VisibilityResult<()> {
        let bytes = self.entry()?;
        if retirement_row(&bytes[..])[0] != 0 {
            return Err(self.origin.fatal(VisibilityCause::Invariant));
        }
        Ok(())
    }
}

fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().expect("fixed registry u64"))
}
fn put_u64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn snapshot_row(bytes: &[u8], index: usize) -> &[u8] {
    &bytes[SNAPSHOT_START + index * SNAPSHOT_BYTES..SNAPSHOT_START + (index + 1) * SNAPSHOT_BYTES]
}
fn snapshot_mut(bytes: &mut [u8], index: usize) -> &mut [u8] {
    &mut bytes
        [SNAPSHOT_START + index * SNAPSHOT_BYTES..SNAPSHOT_START + (index + 1) * SNAPSHOT_BYTES]
}
fn admission_row(bytes: &[u8], index: usize) -> &[u8] {
    &bytes
        [ADMISSION_START + index * ADMISSION_BYTES..ADMISSION_START + (index + 1) * ADMISSION_BYTES]
}
fn admission_mut(bytes: &mut [u8], index: usize) -> &mut [u8] {
    &mut bytes
        [ADMISSION_START + index * ADMISSION_BYTES..ADMISSION_START + (index + 1) * ADMISSION_BYTES]
}
fn reference_row(bytes: &[u8], index: usize) -> &[u8] {
    &bytes
        [REFERENCE_START + index * REFERENCE_BYTES..REFERENCE_START + (index + 1) * REFERENCE_BYTES]
}
fn reference_mut(bytes: &mut [u8], index: usize) -> &mut [u8] {
    &mut bytes
        [REFERENCE_START + index * REFERENCE_BYTES..REFERENCE_START + (index + 1) * REFERENCE_BYTES]
}
fn retirement_row(bytes: &[u8]) -> &[u8] {
    &bytes[RETIREMENT_START..]
}
fn retirement_mut(bytes: &mut [u8]) -> &mut [u8] {
    &mut bytes[RETIREMENT_START..]
}

fn validate(bytes: &[u8; VISIBILITY_BYTES]) -> Result<(), ()> {
    if &bytes[..8] != MAGIC || bytes[16..96].iter().any(|b| *b != 0) {
        return Err(());
    }
    snapshot::validate(bytes)?;
    pins::validate(bytes)?;
    retirement::validate(bytes)
}

/// The reader owns this reservation independently of a peer Commit/CLR loan.
/// Its retained Catalog copy never aliases the prefix borrowed by the loader.
pub(in crate::storage) struct CatalogLookupLoan<'a> {
    pub(in crate::storage) loader: &'a mut [u8; 409_600],
    pub(in crate::storage) catalog: &'a mut [u8; 16_384],
}
impl<'a> CatalogLookupLoan<'a> {
    pub(in crate::storage) fn split(bytes: &'a mut [u8; 425_984]) -> Self {
        let (loader, catalog) = bytes.split_at_mut(409_600);
        Self {
            loader: loader.try_into().expect("reader loader prefix"),
            catalog: catalog.try_into().expect("reader copy tail"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(in crate::storage) struct VisibilitySourceCensus {
    pub(in crate::storage) backing: usize,
    pub(in crate::storage) controller: usize,
    pub(in crate::storage) frontier: usize,
    pub(in crate::storage) snapshot: usize,
    pub(in crate::storage) reference: usize,
    pub(in crate::storage) retirement: usize,
    pub(in crate::storage) peak_leaf: usize,
    pub(in crate::storage) reader_private: usize,
}
/// Source values and carriers, not a compiler-stack estimate. The caller adds
/// its retained controller, original targets, frame/semantic values and loans.
pub(in crate::storage) fn source_census() -> VisibilitySourceCensus {
    use std::mem::size_of;
    let leaf = 2 * size_of::<VisibilityResult<MutexGuard<'_, &mut [u8; VISIBILITY_BYTES]>>>()
        + size_of::<MutexGuard<'_, &mut [u8; VISIBILITY_BYTES]>>()
        + 2 * size_of::<VisibilityResult<MutexGuard<'_, Publication<'_>>>>()
        + 2 * size_of::<VisibilityResult<VersionIdentity>>()
        + 2 * size_of::<VisibilityResult<OwnerIdentity>>()
        + 2 * size_of::<VisibilityError>()
        + 2 * size_of::<authority::ReferenceTarget>()
        + 2 * size_of::<Result<authority::ReferenceTarget, ()>>()
        + 2 * size_of::<Result<UndoRef, crate::storage::page::PageError>>()
        + 2 * size_of::<Result<TxnRef, crate::storage::page::PageError>>()
        + 32 * size_of::<u64>()
        + 128;
    let snapshot = leaf
        + size_of::<SnapshotReservation<'_, '_, '_, '_>>()
        + 2 * size_of::<VisibilityResult<SnapshotReservation<'_, '_, '_, '_>>>()
        + 2 * size_of::<SnapshotGuard<'_, '_, '_, '_>>()
        + 2 * size_of::<VisibilityResult<SnapshotGuard<'_, '_, '_, '_>>>()
        + size_of::<VersionEditGuard<'_, '_, '_>>()
        + 2 * size_of::<crate::transaction::concurrency::Result<VersionEditGuard<'_, '_, '_>>>()
        + size_of::<VisibilityResult<()>>();
    let reference = leaf
        + size_of::<ReferenceLease<'_, '_, '_, '_>>()
        + 2 * size_of::<VisibilityResult<ReferenceLease<'_, '_, '_, '_>>>()
        + size_of::<CapturedCatalogRefs<'_, '_, '_, '_>>()
        + 2 * size_of::<VisibilityResult<CapturedCatalogRefs<'_, '_, '_, '_>>>()
        + size_of::<CatalogSource<'_, '_, '_>>()
        + size_of::<RollbackSource<'_, '_, '_>>()
        + 2 * size_of::<VisibilityResult<UndoPin<'_, '_, '_, '_>>>();
    let retirement = leaf
        + size_of::<HorizonObservation<'_, '_>>()
        + size_of::<VerifiedRetirement<'_, '_>>()
        + size_of::<AppliedRetirement<'_, '_>>()
        + size_of::<StagedRetirement<'_, '_>>()
        + size_of::<Option<WalRequirement<'_>>>()
        + size_of::<RetirementIntent<'_, '_, '_, '_>>()
        + 2 * size_of::<Result<RetirementIntent<'_, '_, '_, '_>, RetirementReject<'_, '_, '_, '_>>>(
        )
        + 2 * size_of::<VisibilityResult<Option<UndoPin<'_, '_, '_, '_>>>>()
        + size_of::<VisibilityResult<u64>>();
    VisibilitySourceCensus {
        backing: VISIBILITY_BYTES + EPOCH_BYTES,
        controller: size_of::<BoundVisibility<'_, '_, '_>>(),
        frontier: size_of::<InstalledFrontier<'_, '_>>(),
        snapshot,
        reference,
        retirement,
        peak_leaf: snapshot.max(reference).max(retirement),
        reader_private: 425_984,
    }
}
pub(in crate::storage) fn fits() -> bool {
    let c = source_census();
    c.controller <= 128 && c.frontier <= 64 && c.peak_leaf <= 4096
}

#[cfg(test)]
impl BoundVisibility<'_, '_, '_> {
    /// Read-only post-fault scalar; no snapshot, frontier or publication authority.
    pub(super) fn test_q(&self) -> u64 {
        self.published
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .q
    }
    /// Observation only: the actual lane identity and activation state.
    pub(super) fn test_epoch_backing(&self) -> (usize, bool) {
        let publication = self.published.lock().unwrap();
        (
            publication.epochs.as_ptr() as usize,
            publication.epochs_active,
        )
    }
    /// Read-only (Q, H) with the capture_horizon min rule. It takes no version
    /// hold, so it observes but never replaces an actual retirement check.
    pub(super) fn test_horizon(&self) -> (u64, u64) {
        let published = self.test_q();
        let bytes = self
            .registry
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let mut horizon = published;
        for index in 0..SNAPSHOTS {
            let row = snapshot_row(&bytes[..], index);
            if row[0] == 2 {
                horizon = horizon.min(u64_at(row, 16));
            }
        }
        (published, horizon)
    }
    /// Source reference pins and whether a retirement marker is published.
    pub(super) fn test_references(&self) -> ((usize, usize, usize), bool) {
        let bytes = self
            .registry
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        (
            pins::test_census(&bytes[..]),
            retirement_row(&bytes[..])[0] != 0,
        )
    }
}
