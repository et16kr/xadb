//! Explicit resource accounting, cooperative cancellation, admission, and diagnostics.
//!
//! Shared state is owned by each handle family; there is no global runtime. Quota uses
//! atomic reservations, lifecycle uses one mutex, and event buffers require exclusive
//! access. Contracts are exercised by `tests/contracts.rs` (reservation, lifecycle,
//! diagnostics, and controlled schedule cases); native effects compose in xadb-platform.
use crate::{Error, ErrorCode, Result, identity::ObjectHandle};
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

/// Shared limit for explicitly accounted units, not total process memory or OOM recovery.
/// Clones share the same atomic ledger; allocation of the handle itself is infallible.
#[derive(Clone)]
pub struct Quota(Arc<QuotaState>);
struct QuotaState {
    limit: u64,
    used: AtomicU64,
    peak: AtomicU64,
}
/// Non-clone lease that returns its accounted units on Drop, without allocation.
/// Keep it alive as long as the associated resource; it does not own that resource.
#[must_use = "keep the reservation alive for the lifetime of the accounted resource"]
pub struct Reservation {
    quota: Quota,
    amount: u64,
}
impl Quota {
    /// Create an empty ledger. A zero limit permits only zero-unit reservations.
    pub fn new(limit: u64) -> Self {
        Self(Arc::new(QuotaState {
            limit,
            used: AtomicU64::new(0),
            peak: AtomicU64::new(0),
        }))
    }
    /// Observe the currently reserved units; concurrent reservations may change it immediately.
    pub fn used(&self) -> u64 {
        self.0.used.load(Ordering::Acquire)
    }
    /// Highest successfully reserved total in this ledger's lifetime, including
    /// reservations released before a collector ran. Exact after reservers join;
    /// during a concurrent reserve this observation may lag the used counter.
    /// Rejected reservations never raise the peak. Clones share the same peak.
    pub fn peak(&self) -> u64 {
        self.0.peak.load(Ordering::Acquire)
    }
    /// Atomically reserve units and return the lease that releases them.
    /// Does not acquire a mutex or wait for units to become available.
    ///
    /// # Errors
    /// Returns [`ErrorCode::QuotaExceeded`] on overflow or insufficient capacity,
    /// without changing the ledger. See `real_threads_cannot_overbook_quota`.
    pub fn reserve(&self, amount: u64) -> Result<Reservation> {
        let previous = self
            .0
            .used
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(amount).filter(|new| *new <= self.0.limit)
            })
            .map_err(|_| Error::new(ErrorCode::QuotaExceeded, "reservation exceeds quota"))?;
        // The successful CAS returns its pre-reservation count. Reading used()
        // here would miss a real peak if another owner released in between.
        self.0.peak.fetch_max(previous + amount, Ordering::AcqRel);
        Ok(Reservation {
            quota: self.clone(),
            amount,
        })
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        self.quota.0.used.fetch_sub(self.amount, Ordering::AcqRel);
    }
}

/// One-way cooperative cancellation shared by clones, initially not cancelled.
/// Checking the token is not atomic with a later effect and does not interrupt syscalls.
#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    /// Request cancellation for all clones. Repeated requests are harmless.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    /// Observe cancellation before a caller-owned effect; never undo an earlier effect.
    ///
    /// # Errors
    /// Returns [`ErrorCode::Cancelled`] if cancellation was observed. A successful
    /// check does not prevent cancellation immediately afterwards. Native thread
    /// visibility is tested in xadb-platform's `cancellation_crosses_threads_and_native_clock_is_monotonic`.
    pub fn check(&self) -> Result<()> {
        if self.0.load(Ordering::Acquire) {
            Err(Error::new(
                ErrorCode::Cancelled,
                "operation cancelled before effect",
            ))
        } else {
            Ok(())
        }
    }
}

/// Admission phase; stopping never reopens the same lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// New operations may enter.
    Running,
    /// New admission is closed while existing guards remain alive.
    Draining,
    /// Admission is closed and all operation guards have been released.
    Stopped,
}
struct State {
    phase: Phase,
    active: u64,
}
/// Admission and drain accounting shared by clones, initially [`Phase::Running`].
/// One private mutex linearizes admission, stop, and guard release. No callback runs
/// under it. Poison fails subsequent public operations closed; restart needs a new instance.
#[derive(Clone)]
pub struct Lifecycle(Arc<Mutex<State>>);
/// Admitted-operation lease. Drop releases accounting, never commits or cancels work.
/// Drop can block on the lifecycle mutex and only repairs accounting on poison;
/// it does not clear poison or permit subsequent service reuse.
#[must_use = "keep the guard alive until the admitted operation finishes"]
pub struct OperationGuard(Lifecycle);
impl Default for Lifecycle {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(State {
            phase: Phase::Running,
            active: 0,
        })))
    }
}
impl Lifecycle {
    /// Admit one operation under the lifecycle mutex and return its lifetime guard.
    ///
    /// # Errors
    /// Returns [`ErrorCode::Closed`] after stop, [`ErrorCode::LimitExceeded`] on
    /// counter overflow, or [`ErrorCode::Poisoned`] after lock poison. No operation
    /// is admitted on failure. See `stop_closes_admission_then_drains_existing_work`.
    pub fn admit(&self) -> Result<OperationGuard> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| Error::new(ErrorCode::Poisoned, "lifecycle lock poisoned"))?;
        if state.phase != Phase::Running {
            return Err(Error::new(ErrorCode::Closed, "admission closed"));
        }
        state.active = state.active.checked_add(1).ok_or(Error::new(
            ErrorCode::LimitExceeded,
            "active operation counter",
        ))?;
        Ok(OperationGuard(self.clone()))
    }
    /// Close admission and report Stopped or Draining without waiting for active work.
    /// Repeated calls preserve closure. The last guard Drop completes Draining;
    /// callers may observe progress with [`Self::phase`]. This call may wait for the mutex.
    ///
    /// # Errors
    /// Returns [`ErrorCode::Poisoned`] if the mutex is poisoned; do not reuse the lifecycle.
    pub fn stop(&self) -> Result<Phase> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| Error::new(ErrorCode::Poisoned, "lifecycle lock poisoned"))?;
        state.phase = if state.active == 0 {
            Phase::Stopped
        } else {
            Phase::Draining
        };
        Ok(state.phase)
    }
    /// Read the phase under the mutex; does not wait for drain completion.
    ///
    /// # Errors
    /// Returns [`ErrorCode::Poisoned`] if the lifecycle cannot be safely observed.
    pub fn phase(&self) -> Result<Phase> {
        Ok(self
            .0
            .lock()
            .map_err(|_| Error::new(ErrorCode::Poisoned, "lifecycle lock poisoned"))?
            .phase)
    }
}
impl Drop for OperationGuard {
    fn drop(&mut self) {
        // Only release accounting on poison; future admission still fails closed.
        let mut state = self
            .0
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.active -= 1;
        if state.active == 0 && state.phase == Phase::Draining {
            state.phase = Phase::Stopped;
        }
    }
}

/// Monotonic nanoseconds in the implementation's local epoch, never wall-clock time.
/// Values from unrelated clock instances need not be comparable. Overflow handling
/// belongs to the implementation; production clocks saturate and manual advance rejects overflow.
pub trait Clock {
    /// Observe time without moving it backwards. No durability or ordering barrier is implied.
    fn now_nanos(&self) -> u64;
}
/// Explicit scheduling checkpoint; implementations define whether they yield or validate.
/// Does not itself enforce thread interleavings. Callers must not hold locks whose
/// progress depends on another actor reaching a checkpoint.
pub trait Scheduler {
    /// Reach a named checkpoint for an actor.
    ///
    /// # Errors
    /// Controlled implementations reject invalid order with [`ErrorCode::InvalidSchedule`]
    /// and poisoned state with [`ErrorCode::Poisoned`]. No product effect is rolled back.
    fn checkpoint(&self, actor: u64, point: &'static str) -> Result<()>;
}
/// Diagnostic observation only; never an independent correctness oracle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    /// Caller-assigned operation correlation ID.
    pub operation_id: u64,
    /// Optional logical object identity at observation time.
    pub object: Option<ObjectHandle>,
    /// Monotonic nanoseconds from the caller's clock epoch.
    pub at_nanos: u64,
    /// Static event identifier owned by the emitting module.
    pub event_id: &'static str,
    /// Optional observed foundation error code.
    pub error: Option<ErrorCode>,
}
/// Diagnostic sink with exclusive mutation; an implementation must document loss behavior.
pub trait EventSink {
    /// Submit an observation. Absence of an error return is not a persistence guarantee.
    fn emit(&mut self, event: Event);
}
/// Preallocated bounded queue. Full queues discard the newest event and count loss.
/// Emit/pop require exclusive access and do not allocate. See `diagnostics_are_bounded_and_report_loss`.
pub struct EventBuffer {
    events: VecDeque<Event>,
    capacity: usize,
    dropped: u64,
}
impl EventBuffer {
    /// Allocate a queue with capacity 1..=4096 events.
    ///
    /// # Errors
    /// Returns [`ErrorCode::LimitExceeded`] outside that range or
    /// [`ErrorCode::QuotaExceeded`] if the backing allocation cannot be reserved.
    pub fn new(capacity: usize) -> Result<Self> {
        if capacity == 0 || capacity > 4096 {
            return Err(Error::new(
                ErrorCode::LimitExceeded,
                "event capacity must be 1..4096",
            ));
        }
        let mut events = VecDeque::new();
        events
            .try_reserve_exact(capacity)
            .map_err(|_| Error::new(ErrorCode::QuotaExceeded, "event buffer allocation"))?;
        Ok(Self {
            events,
            capacity,
            dropped: 0,
        })
    }
    /// Remove the oldest retained event, or return None if empty.
    pub fn pop(&mut self) -> Option<Event> {
        self.events.pop_front()
    }
    /// Number of discarded newest events, saturating at u64::MAX.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
}
impl EventSink for EventBuffer {
    fn emit(&mut self, event: Event) {
        if self.events.len() == self.capacity {
            self.dropped = self.dropped.saturating_add(1);
        } else {
            self.events.push_back(event);
        }
    }
}

#[cfg(feature = "test-support")]
/// Test-only clock and explicit schedule validation; excluded from default builds.
pub mod controlled {
    use super::{AtomicU64, Clock, Error, ErrorCode, Mutex, Ordering, Result, Scheduler, VecDeque};
    /// Atomic clock starting at zero; reads do not advance time.
    #[derive(Default)]
    pub struct ManualClock(AtomicU64);
    impl ManualClock {
        /// Atomically advance by nanoseconds.
        ///
        /// # Errors
        /// Returns [`ErrorCode::Overflow`] without changing time if the sum overflows.
        pub fn advance(&self, nanos: u64) -> Result<()> {
            self.0
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |old| {
                    old.checked_add(nanos)
                })
                .map_err(|_| Error::new(ErrorCode::Overflow, "clock overflow"))?;
            Ok(())
        }
    }
    impl Clock for ManualClock {
        fn now_nanos(&self) -> u64 {
            self.0.load(Ordering::Acquire)
        }
    }

    /// Validates an explicitly driven schedule; does not pretend to explore all thread interleavings.
    struct ScheduleState {
        steps: VecDeque<(u64, &'static str)>,
        violated: bool,
    }
    /// Validates a driver-supplied order under one mutex; does not block awaiting an actor.
    /// First order failure is sticky. `finish` is required to detect unconsumed steps.
    /// See `rejected_schedule_cannot_later_finish_successfully`.
    pub struct ScriptedScheduler(Mutex<ScheduleState>);
    impl ScriptedScheduler {
        /// Take ownership of at most 4096 checkpoints. An empty schedule is already complete.
        ///
        /// # Errors
        /// Returns [`ErrorCode::LimitExceeded`] for longer schedules.
        pub fn new(schedule: Vec<(u64, &'static str)>) -> Result<Self> {
            if schedule.len() > 4096 {
                return Err(Error::new(ErrorCode::LimitExceeded, "schedule budget"));
            }
            Ok(Self(Mutex::new(ScheduleState {
                steps: schedule.into(),
                violated: false,
            })))
        }
        /// Check that every checkpoint was consumed without any previous violation.
        ///
        /// # Errors
        /// Returns [`ErrorCode::InvalidSchedule`] for missing/violated checkpoints,
        /// or [`ErrorCode::Poisoned`] for a poisoned lock. Never resets failed state.
        pub fn finish(&self) -> Result<()> {
            let state = self
                .0
                .lock()
                .map_err(|_| Error::new(ErrorCode::Poisoned, "schedule lock"))?;
            if !state.violated && state.steps.is_empty() {
                Ok(())
            } else {
                Err(Error::new(
                    ErrorCode::InvalidSchedule,
                    "schedule incomplete or previously violated",
                ))
            }
        }
    }
    impl Scheduler for ScriptedScheduler {
        fn checkpoint(&self, actor: u64, point: &'static str) -> Result<()> {
            let mut state = self
                .0
                .lock()
                .map_err(|_| Error::new(ErrorCode::Poisoned, "schedule lock"))?;
            if state.violated || state.steps.front() != Some(&(actor, point)) {
                state.violated = true;
                return Err(Error::new(
                    ErrorCode::InvalidSchedule,
                    "unexpected actor or point",
                ));
            }
            state.steps.pop_front();
            Ok(())
        }
    }
}
