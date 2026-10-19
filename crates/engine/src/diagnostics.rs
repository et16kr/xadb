//! Optional K0 work counters and bounded completion events.
//!
//! This collector owns no clock and never participates in publication. Recording
//! uses atomics and `try_lock`, without allocation, output or waiting. A snapshot
//! observes each atomic separately; it is not a concurrent point-in-time cut.
use crate::{Error, ErrorCode, Result};
use std::{
    mem::size_of,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

/// Fixed number of instrumented API labels, in [`Operation`] order.
pub const OPERATION_COUNT: usize = 11;
/// Maximum configured completion-event count.
pub const MAX_EVENT_CAPACITY: usize = 1024;
/// Engine-owned collector accounting limit, separate from row quota and RSS.
pub const MAX_LOGICAL_BYTES: usize = 64 * 1024;

/// Collection level; existing five substrate metrics remain enabled in all modes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiagnosticsMode {
    /// No collector allocation and no additional observations.
    #[default]
    Off,
    /// Fixed operation counters, without an event queue.
    Always,
    /// Fixed counters plus bounded, lossy completion events.
    Detailed,
}

/// Collector options fixed when the volatile store is constructed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiagnosticsOptions {
    /// Collection level.
    pub mode: DiagnosticsMode,
    /// Event count bound, 0..=1024. Only Detailed allocates a queue.
    pub event_capacity: usize,
}
impl Default for DiagnosticsOptions {
    fn default() -> Self {
        Self {
            mode: DiagnosticsMode::Off,
            event_capacity: 256,
        }
    }
}

/// Stable bounded operation labels; discriminants index counter arrays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Operation {
    /// Create a table.
    Create,
    /// Drop a table.
    Drop,
    /// Insert a row.
    Insert,
    /// Replace a row.
    Replace,
    /// Delete a row.
    Delete,
    /// Acquire a snapshot pin.
    Snapshot,
    /// Resolve a table name in a snapshot.
    Lookup,
    /// Read fixed table schema.
    Schema,
    /// Read an optional row.
    Read,
    /// Open a scan cursor.
    Scan,
    /// Read a cursor page.
    NextPage,
}

/// Cumulative counts; completion includes both successful and failed returns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OperationCounts {
    /// API entries, including calls that subsequently fail.
    pub started: u64,
    /// Ordinary Ok/Err returns; panic is not a completion.
    pub completed: u64,
    /// Err returns, a subset of completed.
    pub failed: u64,
}

/// Individual completion observation, without timing or unbounded labels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiagnosticEvent {
    /// Unique increasing event-attempt sequence; dropped attempts leave gaps.
    pub sequence: u64,
    /// API operation that returned.
    pub operation: Operation,
    /// Actual returned product error, or None for success.
    pub error: Option<ErrorCode>,
}

/// Separate atomic observations, with missing collection explicitly represented.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticsSnapshot {
    /// Fixed configured collection level.
    pub mode: DiagnosticsMode,
    /// Configured event count bound, even when the mode does not collect events.
    pub event_capacity: usize,
    /// Collector struct + Arc header + actual queue capacity times event size.
    /// Excludes returned drain buffers, allocator overhead and process RSS.
    pub logical_bytes: usize,
    /// Fixed [`Operation`] order, None in Off mode.
    pub operations: Option<[OperationCounts; OPERATION_COUNT]>,
    /// Cumulative unrecorded completion attempts, only collected in Detailed.
    pub dropped_events: Option<u64>,
    /// Any counter/sequence overflow; exact deltas are invalid once true.
    pub saturated: Option<bool>,
}

/// Drained completion events. The collector retains its original queue capacity.
pub struct DiagnosticsDrain {
    /// Caller-owned bounded buffer; multiple retained drains are outside the
    /// engine-owned accounting bound.
    pub events: Vec<DiagnosticEvent>,
    /// False when queue contention or poison prevented collection. Non-event
    /// modes return an available empty drain, with their mode identifying absence.
    pub available: bool,
}

struct AtomicCounts {
    started: AtomicU64,
    completed: AtomicU64,
    failed: AtomicU64,
}
impl AtomicCounts {
    fn new() -> Self {
        Self {
            started: AtomicU64::new(0),
            completed: AtomicU64::new(0),
            failed: AtomicU64::new(0),
        }
    }
    fn snapshot(&self) -> OperationCounts {
        OperationCounts {
            started: self.started.load(Ordering::Relaxed),
            completed: self.completed.load(Ordering::Relaxed),
            failed: self.failed.load(Ordering::Relaxed),
        }
    }
}

struct Collector {
    operations: [AtomicCounts; OPERATION_COUNT],
    events: Mutex<Vec<DiagnosticEvent>>,
    // A lossy hint, set/reset only while holding events. A full queue needs no
    // producer lock attempt; draining makes it eligible for recording again.
    full: AtomicBool,
    // Only the queue owner advances emitted. Dropped attempts advance dropped;
    // their sum gives the next queued sequence without a second RMW per drop.
    emitted: AtomicU64,
    dropped: AtomicU64,
    saturated: AtomicBool,
    logical_bytes: usize,
}
impl Collector {
    #[inline]
    fn increment(&self, counter: &AtomicU64) -> Option<u64> {
        match counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        }) {
            Ok(previous) => Some(previous + 1),
            Err(_) => {
                self.saturated.store(true, Ordering::Relaxed);
                None
            }
        }
    }

    fn saturated(&self) -> bool {
        // Emission and dropping use different atomics. Neither producer can
        // reliably detect their combined overflow by reading the other after
        // updating its own counter: both loads can observe the old value.
        // Derive that persistent condition here; all components are monotone,
        // and this API already promises separate observations, not a live cut.
        self.saturated.load(Ordering::Relaxed)
            || self
                .emitted
                .load(Ordering::Relaxed)
                .checked_add(self.dropped.load(Ordering::Relaxed))
                .is_none()
    }
}

#[derive(Clone)]
pub(crate) struct Diagnostics {
    options: DiagnosticsOptions,
    collector: Option<Arc<Collector>>,
}
impl Diagnostics {
    pub(crate) fn new(options: DiagnosticsOptions) -> Result<Self> {
        if options.event_capacity > MAX_EVENT_CAPACITY {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "diagnostics event capacity",
            ));
        }
        if options.mode == DiagnosticsMode::Off {
            return Ok(Self {
                options,
                collector: None,
            });
        }
        let mut events = Vec::new();
        if options.mode == DiagnosticsMode::Detailed {
            events
                .try_reserve_exact(options.event_capacity)
                .map_err(|_| Error::new(ErrorCode::QuotaExceeded, "diagnostics allocation"))?;
        }
        let logical_bytes = events
            .capacity()
            .checked_mul(size_of::<DiagnosticEvent>())
            .and_then(|bytes| bytes.checked_add(size_of::<Collector>() + 2 * size_of::<usize>()))
            .filter(|bytes| *bytes <= MAX_LOGICAL_BYTES)
            .ok_or(Error::new(
                ErrorCode::QuotaExceeded,
                "diagnostics logical bytes",
            ))?;
        Ok(Self {
            options,
            collector: Some(Arc::new(Collector {
                operations: std::array::from_fn(|_| AtomicCounts::new()),
                events: Mutex::new(events),
                full: AtomicBool::new(options.event_capacity == 0),
                emitted: AtomicU64::new(0),
                dropped: AtomicU64::new(0),
                saturated: AtomicBool::new(false),
                logical_bytes,
            })),
        })
    }

    #[inline]
    pub(crate) fn started(&self, operation: Operation) {
        if let Some(collector) = &self.collector {
            collector.increment(&collector.operations[operation as usize].started);
        }
    }

    #[inline]
    pub(crate) fn completed<T>(&self, operation: Operation, result: &Result<T>) {
        let Some(collector) = &self.collector else {
            return;
        };
        let counts = &collector.operations[operation as usize];
        collector.increment(&counts.completed);
        let error = result.as_ref().err().map(|error| error.code);
        if error.is_some() {
            collector.increment(&counts.failed);
        }
        if self.options.mode != DiagnosticsMode::Detailed {
            return;
        }
        if collector.full.load(Ordering::Relaxed) {
            collector.increment(&collector.dropped);
            return;
        }
        self.record_event(collector, operation, error);
    }

    // Keep the queue path out of the common counter-only/full-buffer path.
    fn record_event(&self, collector: &Collector, operation: Operation, error: Option<ErrorCode>) {
        // The queue lock serializes successful event publication. Concurrent
        // drops may appear as gaps in this or a later event, but monotone totals
        // ensure queued sequences never repeat or go backward.
        if let Ok(mut events) = collector.events.try_lock()
            && events.len() < self.options.event_capacity
        {
            let emitted = collector.emitted.load(Ordering::Relaxed);
            let Some(sequence) = emitted
                .checked_add(collector.dropped.load(Ordering::Relaxed))
                .and_then(|attempts| attempts.checked_add(1))
            else {
                collector.saturated.store(true, Ordering::Relaxed);
                collector.increment(&collector.dropped);
                return;
            };
            events.push(DiagnosticEvent {
                sequence,
                operation,
                error,
            });
            collector.emitted.store(emitted + 1, Ordering::Relaxed);
            if events.len() == self.options.event_capacity {
                collector.full.store(true, Ordering::Relaxed);
            }
            return;
        }
        collector.increment(&collector.dropped);
    }

    pub(crate) fn snapshot(&self) -> DiagnosticsSnapshot {
        DiagnosticsSnapshot {
            mode: self.options.mode,
            event_capacity: self.options.event_capacity,
            logical_bytes: self
                .collector
                .as_ref()
                .map_or(0, |collector| collector.logical_bytes),
            operations: self.collector.as_ref().map(|collector| {
                std::array::from_fn(|index| collector.operations[index].snapshot())
            }),
            dropped_events: self
                .collector
                .as_ref()
                .filter(|_| self.options.mode == DiagnosticsMode::Detailed)
                .map(|collector| collector.dropped.load(Ordering::Relaxed)),
            saturated: self
                .collector
                .as_ref()
                .map(|collector| collector.saturated()),
        }
    }

    pub(crate) fn drain(&self) -> Result<DiagnosticsDrain> {
        let Some(collector) = self
            .collector
            .as_ref()
            .filter(|_| self.options.mode == DiagnosticsMode::Detailed)
        else {
            return Ok(DiagnosticsDrain {
                events: Vec::new(),
                available: true,
            });
        };
        let mut output = Vec::new();
        output
            .try_reserve_exact(self.options.event_capacity)
            .map_err(|_| Error::new(ErrorCode::QuotaExceeded, "diagnostics drain allocation"))?;
        if output.capacity() > self.options.event_capacity {
            return Err(Error::new(
                ErrorCode::QuotaExceeded,
                "diagnostics drain capacity",
            ));
        }
        let Ok(mut events) = collector.events.try_lock() else {
            return Ok(DiagnosticsDrain {
                events: output,
                available: false,
            });
        };
        output.extend(events.drain(..));
        collector
            .full
            .store(self.options.event_capacity == 0, Ordering::Relaxed);
        Ok(DiagnosticsDrain {
            events: output,
            available: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detailed() -> Diagnostics {
        Diagnostics::new(DiagnosticsOptions {
            mode: DiagnosticsMode::Detailed,
            event_capacity: 1,
        })
        .unwrap()
    }

    #[test]
    fn contention_and_poison_drop_without_reusing_poisoned_queue() {
        let diagnostics = detailed();
        let collector = diagnostics.collector.as_ref().unwrap();
        let held = collector.events.lock().unwrap();
        diagnostics.completed(Operation::Read, &Ok(()));
        assert!(!diagnostics.drain().unwrap().available);
        drop(held);
        let result = std::panic::catch_unwind(|| {
            let _held = collector.events.lock().unwrap();
            panic!("poison only the isolated diagnostics queue");
        });
        assert!(result.is_err());
        diagnostics.completed(Operation::Read, &Ok(()));
        assert_eq!(diagnostics.snapshot().dropped_events, Some(2));
        assert!(!diagnostics.drain().unwrap().available);
    }

    #[test]
    fn counters_and_sequence_never_wrap_or_duplicate() {
        let diagnostics = detailed();
        let collector = diagnostics.collector.as_ref().unwrap();
        collector.operations[Operation::Read as usize]
            .started
            .store(u64::MAX, Ordering::Relaxed);
        collector.emitted.store(u64::MAX - 1, Ordering::Relaxed);
        diagnostics.started(Operation::Read);
        diagnostics.completed(Operation::Read, &Ok(()));
        let first = diagnostics.drain().unwrap();
        assert_eq!(first.events[0].sequence, u64::MAX);
        diagnostics.completed(Operation::Read, &Ok(()));
        assert!(diagnostics.drain().unwrap().events.is_empty());
        let snapshot = diagnostics.snapshot();
        assert_eq!(snapshot.saturated, Some(true));
        assert_eq!(
            snapshot.operations.unwrap()[Operation::Read as usize].started,
            u64::MAX
        );
        assert_eq!(snapshot.dropped_events, Some(1));
        collector.dropped.store(u64::MAX, Ordering::Relaxed);
        diagnostics.completed(Operation::Read, &Ok(()));
        assert_eq!(diagnostics.snapshot().dropped_events, Some(u64::MAX));
    }

    #[test]
    fn combined_sequence_overflow_is_visible_after_the_last_racing_updates() {
        let diagnostics = detailed();
        let collector = diagnostics.collector.as_ref().unwrap();
        // The two final concurrent attempts can each observe the other total
        // before its update. Neither individual counter has overflowed.
        collector.emitted.store(u64::MAX, Ordering::Relaxed);
        collector.dropped.store(1, Ordering::Relaxed);
        assert!(!collector.saturated.load(Ordering::Relaxed));
        assert_eq!(diagnostics.snapshot().saturated, Some(true));
        diagnostics.completed(Operation::Read, &Ok(()));
        assert!(diagnostics.drain().unwrap().events.is_empty());
        assert_eq!(diagnostics.snapshot().dropped_events, Some(2));
    }

    #[test]
    fn bounded_drains_conserve_concurrent_attempts_and_order_including_saturation() {
        for seed in 0..64 {
            let diagnostics = Diagnostics::new(DiagnosticsOptions {
                mode: DiagnosticsMode::Detailed,
                event_capacity: seed % 4,
            })
            .unwrap();
            let collector = diagnostics.collector.as_ref().unwrap();
            let initial = if seed % 2 == 0 { 0 } else { u64::MAX - 8 };
            collector.emitted.store(initial, Ordering::Relaxed);
            let mut events = Vec::new();
            std::thread::scope(|scope| {
                for operation in [
                    Operation::Read,
                    Operation::Snapshot,
                    Operation::Lookup,
                    Operation::Schema,
                ] {
                    let diagnostics = &diagnostics;
                    scope.spawn(move || {
                        for _ in 0..128 {
                            diagnostics.started(operation);
                            diagnostics.completed(operation, &Ok(()));
                            std::thread::yield_now();
                        }
                    });
                }
                for _ in 0..128 {
                    events.extend(diagnostics.drain().unwrap().events);
                    std::thread::yield_now();
                }
            });
            events.extend(diagnostics.drain().unwrap().events);
            let snapshot = diagnostics.snapshot();
            assert_eq!(snapshot.dropped_events.unwrap() + events.len() as u64, 512);
            assert_eq!(
                collector.emitted.load(Ordering::Relaxed) - initial,
                events.len() as u64
            );
            assert!(events.iter().all(|event| event.sequence > initial));
            assert!(
                events
                    .windows(2)
                    .all(|pair| pair[0].sequence < pair[1].sequence)
            );
            assert_eq!(snapshot.saturated, Some(initial != 0));
            for operation in [
                Operation::Read,
                Operation::Snapshot,
                Operation::Lookup,
                Operation::Schema,
            ] {
                assert_eq!(
                    snapshot.operations.unwrap()[operation as usize],
                    OperationCounts {
                        started: 128,
                        completed: 128,
                        failed: 0,
                    }
                );
            }
        }
    }
}
