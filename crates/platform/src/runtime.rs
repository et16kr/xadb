//! Native time and cooperative scheduling using Rust standard facilities.
use foundation::{
    Result,
    runtime::{Clock, Scheduler},
};
use std::time::Instant;

/// Native monotonic clock with a per-instance Instant epoch.
/// Values saturate at u64::MAX nanoseconds; unrelated instances cannot be compared.
/// Verified by `cancellation_crosses_threads_and_native_clock_is_monotonic`.
pub struct MonotonicClock(Instant);
impl Default for MonotonicClock {
    fn default() -> Self {
        Self(Instant::now())
    }
}
impl Clock for MonotonicClock {
    fn now_nanos(&self) -> u64 {
        self.0.elapsed().as_nanos().min(u64::MAX as u128) as u64
    }
}
/// Cooperative OS thread yield at each checkpoint; actor and point are diagnostic only.
/// Does not validate schedules, guarantee fairness, or control all thread interleavings.
pub struct NativeScheduler;
impl Scheduler for NativeScheduler {
    fn checkpoint(&self, _actor: u64, _point: &'static str) -> Result<()> {
        std::thread::yield_now();
        Ok(())
    }
}
