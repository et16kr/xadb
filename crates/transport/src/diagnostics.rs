//! Fixed-size diagnostic records. No payload or application identifiers are kept.
use crate::{ErrorKind, resources::ClockDomain};
use foundation::network::Registration;
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};

/// The release default retains counters; Detailed additionally records events.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Instrumentation {
    /// Bounded counters and connection aggregates without event-ring writes.
    #[default]
    Always,
    /// The same transport semantics plus a bounded, best-effort event ring.
    Detailed,
}
/// A diagnostic transition, independent of SQL success or peer acknowledgement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    /// A frame entered the local FIFO; value is its wire length.
    Enqueued,
    /// An integrity-checked frame entered the RX queue; value is wire length.
    Received,
    /// A frame completed local writes; value is enqueue-to-flush nanoseconds.
    Flushed,
    /// The connection closed; inspect error for its reason.
    Closed,
}
/// One small record in a transport domain's 256-entry ring.
#[derive(Clone, Copy, Debug)]
pub struct DiagnosticEvent {
    /// Monotonic timestamp in this transport's injected clock domain.
    pub at_nanos: u64,
    /// Exact stream incarnation, never a raw descriptor.
    pub registration: Registration,
    /// Meaning of value.
    pub kind: EventKind,
    /// Wire bytes or nanoseconds, as defined by kind.
    pub value: u64,
    /// Close reason, present only for terminal events.
    pub error: Option<ErrorKind>,
}
/// A bounded copy taken by the assembly after or between driver steps.
#[derive(Clone, Debug)]
pub struct DiagnosticSnapshot {
    /// Oldest to newest retained events followed by empty entries.
    pub events: [Option<DiagnosticEvent>; 256],
    /// Overwritten events and events dropped on recording contention.
    pub lost_events: u64,
    /// Maximum successfully reserved global payload capacity; exact after join.
    pub peak_payload_bytes: u64,
    /// Enqueue rejections due to slot/byte/allocation limits.
    pub backpressured: u64,
}
struct Ring {
    events: [Option<DiagnosticEvent>; 256],
    next: usize,
    len: usize,
}
pub(crate) struct Diagnostics {
    mode: Instrumentation,
    ring: Mutex<Ring>,
    lost: AtomicU64,
    pub(crate) backpressured: AtomicU64,
}
impl Diagnostics {
    pub(crate) fn new(mode: Instrumentation) -> Self {
        Self {
            mode,
            ring: Mutex::new(Ring {
                events: [None; 256],
                next: 0,
                len: 0,
            }),
            lost: AtomicU64::new(0),
            backpressured: AtomicU64::new(0),
        }
    }
    pub(crate) fn record(
        &self,
        clock: &ClockDomain,
        registration: Registration,
        kind: EventKind,
        value: u64,
        error: Option<ErrorKind>,
    ) {
        if self.mode != Instrumentation::Detailed {
            return;
        }
        // Do not wait for a collector or hold the admission gate while recording.
        let Ok(mut ring) = self.ring.try_lock() else {
            increment(&self.lost);
            return;
        };
        let Some(at_nanos) = clock.diagnostic_now() else {
            increment(&self.lost);
            return;
        };
        let next = ring.next;
        ring.events[next] = Some(DiagnosticEvent {
            at_nanos,
            registration,
            kind,
            value,
            error,
        });
        ring.next = (next + 1) % 256;
        if ring.len == 256 {
            increment(&self.lost);
        } else {
            ring.len += 1;
        }
    }
    pub(crate) fn snapshot(&self) -> DiagnosticSnapshot {
        let ring = self
            .ring
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut events = [None; 256];
        for (i, event) in events.iter_mut().take(ring.len).enumerate() {
            *event = ring.events[(ring.next + 256 - ring.len + i) % 256];
        }
        DiagnosticSnapshot {
            events,
            lost_events: self.lost.load(Ordering::Relaxed),
            peak_payload_bytes: 0, // Filled from the shared quota by Transport.
            backpressured: self.backpressured.load(Ordering::Relaxed),
        }
    }
}
pub(crate) fn increment(counter: &AtomicU64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
        Some(n.saturating_add(1))
    });
}

/// Bounded log2 nanosecond distribution; bucket 0 includes zero and one ns.
/// Counts are observations, not independent experiment runs.
#[derive(Clone, Copy, Debug)]
pub struct LatencyHistogram {
    /// Bucket i covers 2^i through 2^(i+1)-1 ns (bucket 63 includes u64::MAX).
    pub buckets: [u64; 64],
    /// At least one bucket saturated.
    pub overflowed: bool,
}
impl Default for LatencyHistogram {
    fn default() -> Self {
        Self {
            buckets: [0; 64],
            overflowed: false,
        }
    }
}
impl LatencyHistogram {
    pub(crate) fn observe(&mut self, nanos: u64) {
        let index = (63 - nanos.max(1).leading_zeros()) as usize;
        let slot = &mut self.buckets[index];
        self.overflowed |= *slot == u64::MAX;
        *slot = slot.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostic_control_storage_fits_driver_budget() {
        // Driver connection slots/epochs/listeners and wait interest/event arrays.
        let driver_control = 128 * (8 + 8 + 32 + 32) + 1024;
        assert!(std::mem::size_of::<Diagnostics>() + driver_control <= 65_536);
    }
}
