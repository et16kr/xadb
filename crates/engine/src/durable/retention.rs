//! Bounded retention bookkeeping. Callers acquire coordinator before this ledger
//! and keep unlink/directory I/O outside it. Drop only releases pin bookkeeping.
use super::{
    codec::MAX_FILES,
    error::{DurableError as Error, DurableErrorCode, Result},
    observation::Observations,
};
use crate::storage::volatile::ARC_HEADER_BYTES;
use foundation::{
    namespace::FileName,
    runtime::{Quota, Reservation},
};
use std::{
    mem::size_of,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
};

/// Separate single-owner system pin reservations, independent of user snapshots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PinKind {
    Checkpoint,
    Backup,
    Selection,
}
impl PinKind {
    const fn index(self) -> usize {
        match self {
            Self::Checkpoint => 0,
            Self::Backup => 1,
            Self::Selection => 2,
        }
    }
}

struct Entry {
    name: FileName,
    pins: u32,
    retiring: bool,
}
struct Ledger {
    entries: Vec<Entry>,
    kinds: [bool; 3],
    started: [Option<u64>; 3],
}

pub(crate) struct Retention {
    ledger: Mutex<Ledger>,
    resident: Quota,
    poisoned: AtomicBool,
    observations: Arc<Observations>,
    _charge: Reservation,
    _extra_capacity: Option<Reservation>,
}

/// Owns one system slot and all registered file pins until explicit drop. Cloning
/// the originating Engine must not clone or silently release this reservation.
pub(crate) struct Pin {
    owner: Arc<Retention>,
    kind: PinKind,
    files: Vec<FileName>,
    _charge: Reservation,
}

impl Retention {
    /// Allocate all ledger entry capacity and charge its actual retained size to
    /// the same resident quota used by committed roots and retained metadata.
    #[cfg(test)]
    pub(crate) fn new(resident: &Quota) -> Result<Arc<Self>> {
        let observations = Observations::new(false, None, resident)?;
        Self::new_with_observations(resident, observations)
    }

    pub(crate) fn new_with_observations(
        resident: &Quota,
        observations: Arc<Observations>,
    ) -> Result<Arc<Self>> {
        let requested = allocation_bytes::<Entry>(MAX_FILES, size_of::<Self>() + ARC_HEADER_BYTES)?;
        let charge = resident.reserve(requested).map_err(Error::foundation)?;
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(MAX_FILES)
            .map_err(|_| Error::allocation())?;
        let extra = allocation_bytes::<Entry>(entries.capacity() - MAX_FILES, 0)?;
        let extra_capacity = if extra == 0 {
            None
        } else {
            Some(resident.reserve(extra).map_err(Error::foundation)?)
        };
        Ok(Arc::new(Self {
            ledger: Mutex::new(Ledger {
                entries,
                kinds: [false; 3],
                started: [None; 3],
            }),
            resident: resident.clone(),
            poisoned: AtomicBool::new(false),
            observations,
            _charge: charge,
            _extra_capacity: extra_capacity,
        }))
    }

    /// Register a complete closure atomically. Input storage is adopted and
    /// charged before locking; every check precedes the first ledger mutation.
    /// The caller holds coordinator so selection/retirement cannot change closure.
    pub(crate) fn pin(self: &Arc<Self>, kind: PinKind, mut files: Vec<FileName>) -> Result<Pin> {
        if files.len() > MAX_FILES {
            return Err(Error::limit("retention pin file count"));
        }
        let charge = self
            .resident
            .reserve(allocation_bytes::<FileName>(files.capacity(), 0)?)
            .map_err(Error::foundation)?;
        files.sort_unstable();
        if files.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(Error::invalid("duplicate retention pin file"));
        }
        let mut ledger = self.lock()?;
        // Same clock domain as durability spans. This diagnostic read is before
        // validation/mutation and cannot turn a clock failure into a pin failure.
        let started = self.observations.timestamp();
        if ledger.kinds[kind.index()] {
            return Err(Error::new(
                DurableErrorCode::Busy,
                "system pin kind already held",
            ));
        }
        let mut missing = 0;
        for name in &files {
            if let Some(entry) = ledger.entries.iter().find(|entry| entry.name == *name) {
                if entry.retiring {
                    return Err(Error::new(DurableErrorCode::Busy, "file is retiring"));
                }
                if entry.pins == u32::MAX {
                    return Err(Error::limit("retention pin counter exhausted"));
                }
            } else {
                missing += 1;
            }
        }
        if ledger
            .entries
            .len()
            .checked_add(missing)
            .is_none_or(|count| count > MAX_FILES)
        {
            return Err(Error::limit("retention ledger capacity"));
        }
        for name in &files {
            if let Some(entry) = ledger.entries.iter_mut().find(|entry| entry.name == *name) {
                // The complete preflight proved this addition; no fallible work
                // or callback separates validation from mutation under this lock.
                entry.pins += 1;
            } else {
                ledger.entries.push(Entry {
                    name: *name,
                    pins: 1,
                    retiring: false,
                });
            }
        }
        ledger.kinds[kind.index()] = true;
        ledger.started[kind.index()] = started;
        drop(ledger);
        Ok(Pin {
            owner: Arc::clone(self),
            kind,
            files,
            _charge: charge,
        })
    }

    /// Mark one unselected, unpinned name Retiring. Unknown orphan names receive
    /// an entry first, using the preallocated capacity. The caller holds coordinator
    /// and supplies the exact current/previous/other live selector closure.
    pub(crate) fn retire(&self, name: FileName, live: &[FileName]) -> Result<bool> {
        let mut ledger = self.lock()?;
        if live.contains(&name) {
            return Ok(false);
        }
        if let Some(entry) = ledger.entries.iter_mut().find(|entry| entry.name == name) {
            if entry.pins != 0 || entry.retiring {
                return Ok(false);
            }
            entry.retiring = true;
        } else {
            if ledger.entries.len() == MAX_FILES {
                return Err(Error::limit("retention orphan capacity"));
            }
            ledger.entries.push(Entry {
                name,
                pins: 0,
                retiring: true,
            });
        }
        Ok(true)
    }

    /// Remove bookkeeping only after the caller confirmed both unlink and the
    /// following directory sync. Any I/O failure leaves Retiring sticky and bars
    /// new pins. This method must run under the caller's coordinator boundary.
    pub(crate) fn retired(&self, name: FileName) -> Result<()> {
        let mut ledger = self.lock()?;
        let index = ledger
            .entries
            .iter()
            .position(|entry| entry.name == name)
            .ok_or(Error::invalid("retired file is absent from ledger"))?;
        if !ledger.entries[index].retiring || ledger.entries[index].pins != 0 {
            return Err(Error::invalid("retired file still live or pinned"));
        }
        ledger.entries.swap_remove(index);
        Ok(())
    }

    /// A poisoned ledger permits existing pin destruction for accounting only.
    /// Callers fence the Engine; this flag never authorizes continued selection/GC.
    pub(crate) fn is_poisoned(&self) -> bool {
        let poisoned = self.poisoned.load(Ordering::Acquire) || self.ledger.is_poisoned();
        if poisoned {
            self.poisoned.store(true, Ordering::Release);
        }
        poisoned
    }

    /// Age since registration began under the ledger lock. This is diagnostic
    /// only: no pin expires and no file becomes reclaimable due to its age.
    /// Empty active set has no oldest age and a measured over-60s count of zero.
    pub(crate) fn pin_diagnostics(&self) -> Result<(Option<u64>, Option<u64>)> {
        let (kinds, started) = {
            let ledger = self.lock()?;
            (ledger.kinds, ledger.started)
        };
        let Some(now) = self.observations.timestamp() else {
            return Ok((None, None));
        };
        let mut oldest = None;
        let mut over = 0_u64;
        for index in 0..kinds.len() {
            if !kinds[index] {
                continue;
            }
            let Some(start) = started[index] else {
                return Ok((None, None));
            };
            let Some(age) = now.checked_sub(start) else {
                self.observations.invalidate_clock();
                return Ok((None, None));
            };
            oldest = Some(oldest.map_or(age, |prior: u64| prior.max(age)));
            if age > 60_000_000_000 {
                over += 1;
            }
        }
        Ok((oldest, Some(over)))
    }

    fn lock(&self) -> Result<MutexGuard<'_, Ledger>> {
        if self.is_poisoned() {
            return Err(poison_error());
        }
        let ledger = self.ledger.lock().map_err(|_| {
            self.poisoned.store(true, Ordering::Release);
            poison_error()
        })?;
        if self.is_poisoned() {
            return Err(poison_error());
        }
        Ok(ledger)
    }
}

impl Pin {
    /// Borrow the exact sorted closure while its pin remains held.
    #[cfg(test)]
    pub(crate) fn files(&self) -> &[FileName] {
        &self.files
    }
}
impl Drop for Pin {
    fn drop(&mut self) {
        let mut ledger = self.owner.ledger.lock().unwrap_or_else(|poison| {
            self.owner.poisoned.store(true, Ordering::Release);
            poison.into_inner()
        });
        if !ledger.kinds[self.kind.index()] {
            self.owner.poisoned.store(true, Ordering::Release);
        }
        for name in &self.files {
            match ledger.entries.iter_mut().find(|entry| entry.name == *name) {
                Some(entry) => {
                    if let Some(pins) = entry.pins.checked_sub(1) {
                        entry.pins = pins;
                    } else {
                        self.owner.poisoned.store(true, Ordering::Release);
                    }
                }
                None => {
                    self.owner.poisoned.store(true, Ordering::Release);
                }
            }
        }
        ledger.kinds[self.kind.index()] = false;
        ledger.started[self.kind.index()] = None;
        // A pin may name checkpoint files that were never created. Unpinned,
        // nonretiring bookkeeping has no persistence obligation and must not
        // accumulate after failed builds. Retiring entries stay sticky until
        // confirmed unlink+directory sync calls retired(). This retains capacity
        // without allocation, filesystem effects, or touching another live pin.
        ledger
            .entries
            .retain(|entry| entry.pins != 0 || entry.retiring);
    }
}

fn allocation_bytes<T>(capacity: usize, fixed: usize) -> Result<u64> {
    let bytes = capacity
        .checked_mul(size_of::<T>())
        .and_then(|bytes| bytes.checked_add(fixed))
        .ok_or(Error::limit("retention allocation overflow"))?;
    u64::try_from(bytes).map_err(|_| Error::limit("retention allocation width"))
}
fn poison_error() -> Error {
    Error {
        fenced: true,
        ..Error::new(DurableErrorCode::Fenced, "retention ledger poisoned")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(index: usize) -> FileName {
        FileName::new(&format!("chunk-{index:016x}-00000000.axf")).unwrap()
    }

    #[test]
    fn overlapping_pin_kinds_block_retirement_until_all_owners_release() {
        let resident = Quota::new(64 * 1024 * 1024);
        let retention = Retention::new(&resident).unwrap();
        let baseline = resident.used();
        let checkpoint = retention
            .pin(PinKind::Checkpoint, vec![name(1), name(2)])
            .unwrap();
        let backup = retention.pin(PinKind::Backup, vec![name(2)]).unwrap();
        assert_eq!(checkpoint.files(), &[name(1), name(2)]);
        assert!(resident.used() > baseline);
        assert!(!retention.retire(name(2), &[]).unwrap());
        drop(checkpoint);
        assert!(!retention.retire(name(2), &[]).unwrap());
        assert!(!retention.retire(name(1), &[name(1)]).unwrap());
        assert!(retention.retire(name(1), &[]).unwrap());
        assert_eq!(
            retention
                .pin(PinKind::Selection, vec![name(1)])
                .err()
                .unwrap()
                .code,
            DurableErrorCode::Busy
        );
        retention.retired(name(1)).unwrap();
        drop(backup);
        assert_eq!(resident.used(), baseline);
        assert!(retention.retire(name(2), &[]).unwrap());
        retention.retired(name(2)).unwrap();
        assert!(retention.ledger.lock().unwrap().entries.is_empty());
        drop(retention);
        assert_eq!(resident.used(), 0);
    }

    #[test]
    fn failed_pin_preflight_never_adds_entries_or_consumes_kind_and_quota() {
        let resident = Quota::new(64 * 1024 * 1024);
        let retention = Retention::new(&resident).unwrap();
        assert!(retention.retire(name(2), &[]).unwrap());
        let baseline = resident.used();
        assert_eq!(
            retention
                .pin(PinKind::Checkpoint, vec![name(1), name(2)])
                .err()
                .unwrap()
                .code,
            DurableErrorCode::Busy
        );
        assert_eq!(resident.used(), baseline);
        let ledger = retention.ledger.lock().unwrap();
        assert_eq!(ledger.entries.len(), 1);
        assert_eq!(ledger.entries[0].name, name(2));
        assert!(!ledger.kinds[PinKind::Checkpoint.index()]);
        drop(ledger);
        assert_eq!(
            retention
                .pin(PinKind::Checkpoint, vec![name(1), name(1)])
                .err()
                .unwrap()
                .code,
            DurableErrorCode::Corruption
        );
        let pin = retention.pin(PinKind::Checkpoint, vec![name(1)]).unwrap();
        assert_eq!(
            retention
                .pin(PinKind::Checkpoint, vec![name(3)])
                .err()
                .unwrap()
                .code,
            DurableErrorCode::Busy
        );
        drop(pin);
        assert_eq!(resident.used(), baseline);
    }

    #[test]
    fn pin_drop_removes_unused_names_but_preserves_retiring_entries() {
        let resident = Quota::new(64 * 1024 * 1024);
        let retention = Retention::new(&resident).unwrap();
        let pin = retention.pin(PinKind::Selection, vec![name(1)]).unwrap();
        drop(pin);
        assert!(retention.ledger.lock().unwrap().entries.is_empty());
        assert!(retention.retired(name(1)).is_err());
        assert!(retention.retire(name(1), &[]).unwrap());
        // An external unlink/sync failure deliberately does not call retired().
        assert!(!retention.retire(name(1), &[]).unwrap());
        assert!(retention.pin(PinKind::Backup, vec![name(1)]).is_err());
        // Another pin dropping cannot discard the sticky retirement marker.
        let unrelated = retention.pin(PinKind::Checkpoint, vec![name(2)]).unwrap();
        drop(unrelated);
        let ledger = retention.ledger.lock().unwrap();
        assert_eq!(ledger.entries.len(), 1);
        assert_eq!(ledger.entries[0].name, name(1));
        assert!(ledger.entries[0].retiring);
        drop(ledger);
        retention.retired(name(1)).unwrap();
        assert!(retention.ledger.lock().unwrap().entries.is_empty());
    }

    #[test]
    fn repeated_uncreated_checkpoint_closures_do_not_exhaust_retention_capacity() {
        let resident = Quota::new(64 * 1024 * 1024);
        let retention = Retention::new(&resident).unwrap();
        let baseline = resident.used();
        for generation in 0..3 {
            let files = (1..=MAX_FILES)
                .map(|index| name(generation * MAX_FILES + index))
                .collect();
            let pin = retention.pin(PinKind::Checkpoint, files).unwrap();
            // Failure before any filesystem create leaves no inventory entry
            // for later GC. Drop itself must return these metadata slots.
            drop(pin);
            assert!(retention.ledger.lock().unwrap().entries.is_empty());
            assert_eq!(resident.used(), baseline);
        }
    }

    #[test]
    fn fixed_capacity_rejects_new_orphan_without_losing_existing_entries() {
        let resident = Quota::new(64 * 1024 * 1024);
        let retention = Retention::new(&resident).unwrap();
        let pin = retention
            .pin(PinKind::Checkpoint, (1..=MAX_FILES).map(name).collect())
            .unwrap();
        let error = retention.retire(name(MAX_FILES + 1), &[]).unwrap_err();
        assert_eq!(error.code, DurableErrorCode::LimitExceeded);
        assert_eq!(retention.ledger.lock().unwrap().entries.len(), MAX_FILES);
        drop(pin);
        assert!(retention.retire(name(1), &[]).unwrap());
        retention.retired(name(1)).unwrap();
        assert!(retention.retire(name(MAX_FILES + 1), &[]).unwrap());
    }

    #[test]
    fn poison_allows_accounting_drop_but_fences_all_new_ledger_operations() {
        let resident = Quota::new(64 * 1024 * 1024);
        let retention = Retention::new(&resident).unwrap();
        let baseline = resident.used();
        let pin = retention.pin(PinKind::Backup, vec![name(1)]).unwrap();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = retention.ledger.lock().unwrap();
            panic!("retention poison fixture");
        }));
        assert!(retention.is_poisoned());
        drop(pin);
        assert_eq!(resident.used(), baseline);
        let error = retention.retire(name(1), &[]).unwrap_err();
        assert!(error.fenced);
        assert_eq!(error.code, DurableErrorCode::Fenced);
        assert!(
            retention
                .pin(PinKind::Checkpoint, vec![])
                .err()
                .unwrap()
                .fenced
        );
        assert!(retention.retired(name(1)).unwrap_err().fenced);
        let ledger = retention
            .ledger
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(ledger.entries.is_empty());
        assert!(!ledger.kinds[PinKind::Backup.index()]);
    }
}
