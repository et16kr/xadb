use crate::{
    DiagnosticSnapshot, ErrorKind, Instrumentation, MAX_CONNECTIONS, MAX_WIRE_BYTES,
    PROCESS_PAYLOAD_BYTES, Result, diagnostics::Diagnostics,
};
use std::sync::{Arc, Mutex, atomic::AtomicU64};
use xadb_foundation::runtime::{Clock, Quota, Reservation};

pub(crate) struct ClockDomain {
    clock: Arc<dyn Clock + Send + Sync>,
    last: Mutex<(u64, bool)>,
}
impl ClockDomain {
    // Diagnostics use the last semantic clock sample, without taking another
    // clock observation or changing error/deadline behavior (including in Drop).
    pub(crate) fn diagnostic_now(&self) -> Option<u64> {
        self.last.try_lock().ok().map(|last| last.0)
    }
    pub(crate) fn now(&self) -> Result<u64> {
        let mut last = self.last.lock().map_err(|_| ErrorKind::Poisoned)?;
        let now = self.clock.now_nanos();
        if last.1 || now < last.0 {
            last.1 = true;
            return Err(ErrorKind::AdapterViolation.into());
        }
        last.0 = now;
        Ok(now)
    }
}

/// Shared C1 resource and clock domain. Construct one per process payload budget;
/// all its connections, drivers, and senders share this exact clock instance.
/// Handle allocation is not covered by payload quota and is not an OOM guarantee.
#[derive(Clone)]
pub struct Transport {
    pub(crate) clock: Arc<ClockDomain>,
    payload: Quota,
    connections: Quota,
    pub(crate) diagnostics: Arc<Diagnostics>,
    #[cfg(feature = "test-support")]
    failure: Arc<crate::test_support::Failure>,
    #[cfg(feature = "test-support")]
    model_tx_slots: u64,
}
impl Transport {
    /// Create the fixed 64 MiB / 128-connection domain with no payload reservation.
    pub fn new(clock: Arc<dyn Clock + Send + Sync>) -> Self {
        Self::with_instrumentation(clock, Instrumentation::Always)
    }
    /// Use the same bounded semantics with optional best-effort detailed events.
    pub fn with_instrumentation(
        clock: Arc<dyn Clock + Send + Sync>,
        mode: Instrumentation,
    ) -> Self {
        Self {
            clock: Arc::new(ClockDomain {
                clock,
                last: Mutex::new((0, false)),
            }),
            payload: Quota::new(PROCESS_PAYLOAD_BYTES),
            connections: Quota::new(MAX_CONNECTIONS as u64),
            diagnostics: Arc::new(Diagnostics::new(mode)),
            #[cfg(feature = "test-support")]
            failure: Arc::default(),
            #[cfg(feature = "test-support")]
            model_tx_slots: 4,
        }
    }
    /// Currently charged backing capacity, including external receive leases.
    pub fn payload_bytes(&self) -> u64 {
        self.payload.used()
    }
    /// Live connection families, including closed owners/leases/senders/tickets.
    pub fn connection_count(&self) -> u64 {
        self.connections.used()
    }
    /// Read a bounded diagnostic snapshot. Intended for the assembly/collector;
    /// this method may briefly wait for an event writer, never for socket I/O.
    pub fn diagnostics(&self) -> DiagnosticSnapshot {
        let mut snapshot = self.diagnostics.snapshot();
        snapshot.peak_payload_bytes = self.payload.peak();
        snapshot
    }
    /// Arm one domain-local failure. Only available in non-shipping test builds.
    #[cfg(feature = "test-support")]
    pub fn fail_next(&self, point: crate::test_support::FailurePoint) {
        self.failure.arm(point);
    }
    /// Construct the frozen reduced-model domain with two RX and two TX slots.
    /// Byte/connection limits and every production state transition are retained.
    /// Only test builds expose this constructor; normal builds always use four TX
    /// slots. This avoids a caller-side admission cap masking quota defects.
    #[cfg(feature = "test-support")]
    pub fn for_two_slot_model(clock: Arc<dyn Clock + Send + Sync>) -> Self {
        let mut transport = Self::new(clock);
        transport.model_tx_slots = 2;
        transport
    }
    pub(crate) fn reserve_connection(&self) -> Result<Arc<Family>> {
        #[cfg(feature = "test-support")]
        if self
            .failure
            .take(crate::test_support::FailurePoint::ConnectionSlot)
        {
            return Err(ErrorKind::ResourceExhausted.into());
        }
        let slot = self
            .connections
            .reserve(1)
            .map_err(|_| ErrorKind::ResourceExhausted)?;
        Ok(Arc::new(Family {
            _slot: slot,
            global: self.payload.clone(),
            rx: Quota::new((2 * MAX_WIRE_BYTES) as u64),
            tx: Quota::new((4 * MAX_WIRE_BYTES) as u64),
            rx_slots: Quota::new(2),
            tx_slots: Quota::new({
                #[cfg(feature = "test-support")]
                {
                    self.model_tx_slots
                }
                #[cfg(not(feature = "test-support"))]
                {
                    4
                }
            }),
            diagnostics: self.diagnostics.clone(),
            copied_payload_bytes: AtomicU64::new(0),
            #[cfg(feature = "test-support")]
            failure: self.failure.clone(),
        }))
    }
}
pub(crate) struct Family {
    _slot: Reservation,
    pub(crate) global: Quota,
    pub(crate) rx: Quota,
    pub(crate) tx: Quota,
    pub(crate) rx_slots: Quota,
    pub(crate) tx_slots: Quota,
    pub(crate) diagnostics: Arc<Diagnostics>,
    pub(crate) copied_payload_bytes: AtomicU64,
    #[cfg(feature = "test-support")]
    pub(crate) failure: Arc<crate::test_support::Failure>,
}
pub(crate) struct Buffer {
    pub(crate) bytes: Vec<u8>,
    _local: Reservation,
    _global: Reservation,
    _extra_local: Option<Reservation>,
    _extra_global: Option<Reservation>,
}
impl Buffer {
    /// Reserve before allocation, then account any allocator capacity rounding.
    pub(crate) fn allocate(family: &Family, rx: bool, size: usize) -> Result<Self> {
        #[cfg(feature = "test-support")]
        if family
            .failure
            .take(crate::test_support::FailurePoint::LocalBytes)
        {
            return Err(ErrorKind::ResourceExhausted.into());
        }
        let local = if rx { &family.rx } else { &family.tx };
        let local_charge = local
            .reserve(size as u64)
            .map_err(|_| ErrorKind::ResourceExhausted)?;
        #[cfg(feature = "test-support")]
        if family
            .failure
            .take(crate::test_support::FailurePoint::GlobalBytes)
        {
            return Err(ErrorKind::ResourceExhausted.into());
        }
        let global_charge = family
            .global
            .reserve(size as u64)
            .map_err(|_| ErrorKind::ResourceExhausted)?;
        let mut bytes = Vec::new();
        #[cfg(feature = "test-support")]
        if family
            .failure
            .take(crate::test_support::FailurePoint::Allocation)
        {
            return Err(ErrorKind::ResourceExhausted.into());
        }
        bytes
            .try_reserve_exact(size)
            .map_err(|_| ErrorKind::ResourceExhausted)?;
        let extra = (bytes.capacity() - size) as u64;
        let extra_local = if extra > 0 {
            Some(
                local
                    .reserve(extra)
                    .map_err(|_| ErrorKind::ResourceExhausted)?,
            )
        } else {
            None
        };
        let extra_global = if extra > 0 {
            Some(
                family
                    .global
                    .reserve(extra)
                    .map_err(|_| ErrorKind::ResourceExhausted)?,
            )
        } else {
            None
        };
        bytes.resize(size, 0);
        Ok(Self {
            bytes,
            _local: local_charge,
            _global: global_charge,
            _extra_local: extra_local,
            _extra_global: extra_global,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn connection_control_layout_fits_fixed_budget() {
        // Include the shared gate, family, quota ledgers and Arc headers; frame
        // backing capacity is charged separately. No per-frame heap result cells.
        let bytes = std::mem::size_of::<crate::Connection>()
            + std::mem::size_of::<crate::sender::Shared>()
            + std::mem::size_of::<Family>()
            + 5 * 32
            + 4 * 16;
        assert!(bytes <= 4096, "connection control bytes={bytes}");
    }
}
