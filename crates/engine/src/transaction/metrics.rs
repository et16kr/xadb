//! Fixed counters and optional caller-supplied clock observations. No collection
//! callback acquires the coordinator or publication lock.
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
};
use xadb_foundation::runtime::Clock;

pub(super) type TransactionClock = Arc<dyn Clock + Send + Sync>;

pub(super) struct TransactionCounters {
    clock: Option<TransactionClock>,
    pub acquisitions: AtomicU64,
    wait_ns: AtomicU64,
    hold_ns: AtomicU64,
    invalid_clock: AtomicBool,
    saturated: AtomicBool,
    pub conflicts: AtomicU64,
    pub statement_rollbacks: AtomicU64,
    pub preparation_failures: AtomicU64,
    pins: [PinClock; 32],
}
struct PinClock {
    // 0 free, 1 acquisition in progress, 2 observable. Publishing state last
    // prevents an observer from mistaking an uninitialized timestamp for zero.
    state: AtomicU8,
    acquired: AtomicU64,
    valid: AtomicBool,
}
impl TransactionCounters {
    pub fn new(clock: Option<TransactionClock>) -> Self {
        Self {
            clock,
            acquisitions: AtomicU64::new(0),
            wait_ns: AtomicU64::new(0),
            hold_ns: AtomicU64::new(0),
            invalid_clock: AtomicBool::new(false),
            saturated: AtomicBool::new(false),
            conflicts: AtomicU64::new(0),
            statement_rollbacks: AtomicU64::new(0),
            preparation_failures: AtomicU64::new(0),
            pins: std::array::from_fn(|_| PinClock {
                state: AtomicU8::new(0),
                acquired: AtomicU64::new(0),
                valid: AtomicBool::new(false),
            }),
        }
    }
    pub fn pin(&self) -> Option<usize> {
        for (index, pin) in self.pins.iter().enumerate() {
            if pin
                .state
                .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                let now = self.now();
                pin.acquired.store(now.unwrap_or(0), Ordering::Relaxed);
                pin.valid.store(now.is_some(), Ordering::Relaxed);
                pin.state.store(2, Ordering::Release);
                return Some(index);
            }
        }
        None
    }
    pub fn unpin(&self, index: usize) {
        self.pins[index].state.store(0, Ordering::Release);
    }
    pub fn pin_ages(&self) -> (Option<u64>, Option<u64>) {
        if self.clock.is_none() || self.invalid_clock.load(Ordering::Relaxed) {
            return (None, None);
        }
        let mut starts = [None; 32];
        for (index, pin) in self.pins.iter().enumerate() {
            match pin.state.load(Ordering::Acquire) {
                0 => {}
                2 if pin.valid.load(Ordering::Relaxed) => {
                    starts[index] = Some(pin.acquired.load(Ordering::Relaxed))
                }
                _ => return (None, None), // Acquisition is concurrently incomplete.
            }
        }
        let Some(now) = self.now() else {
            return (None, None);
        };
        let mut oldest = None;
        let mut overdue = 0;
        for start in starts.into_iter().flatten() {
            let Some(age) = now.checked_sub(start) else {
                self.invalidate_clock();
                return (None, None);
            };
            oldest = Some(oldest.map_or(age, |prior: u64| prior.max(age)));
            if age > 60_000_000_000 {
                overdue += 1;
            }
        }
        if self.invalid_clock.load(Ordering::Relaxed) {
            return (None, None);
        }
        (oldest, Some(overdue))
    }
    pub fn now(&self) -> Option<u64> {
        self.clock.as_ref().and_then(|clock| {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| clock.now_nanos())) {
                Ok(now) => Some(now),
                // A diagnostic clock cannot poison transaction state. Only the
                // optional observation is lost; product hook panics still unwind.
                Err(_) => {
                    self.invalidate_clock();
                    None
                }
            }
        })
    }
    pub fn add(&self, counter: &AtomicU64, count: u64) {
        let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
            Some(old.checked_add(count).unwrap_or_else(|| {
                self.saturated.store(true, Ordering::Relaxed);
                u64::MAX
            }))
        });
    }
    fn elapsed(&self, counter: &AtomicU64, start: Option<u64>, end: Option<u64>) {
        if let (Some(start), Some(end)) = (start, end) {
            if let Some(elapsed) = end.checked_sub(start) {
                self.add(counter, elapsed);
            } else {
                self.invalidate_clock();
            }
        }
    }
    pub fn acquired(&self, start: Option<u64>, acquired: Option<u64>) {
        self.add(&self.acquisitions, 1);
        self.elapsed(&self.wait_ns, start, acquired);
    }
    pub fn released(&self, start: Option<u64>) {
        self.elapsed(&self.hold_ns, start, self.now());
    }
    pub fn invalidate_clock(&self) {
        self.invalid_clock.store(true, Ordering::Relaxed);
    }
    pub fn timing(&self) -> (Option<u64>, Option<u64>, bool, bool) {
        let invalid = self.invalid_clock.load(Ordering::Relaxed);
        let available = self.clock.is_some() && !invalid;
        (
            available.then(|| self.wait_ns.load(Ordering::Relaxed)),
            available.then(|| self.hold_ns.load(Ordering::Relaxed)),
            invalid,
            self.saturated.load(Ordering::Relaxed),
        )
    }
}
