//! Test-only, domain-local one-shot failures; absent from default release builds.
use std::sync::atomic::{AtomicU8, Ordering};

/// Failure stage, consumed only by the matching operation in this domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum FailurePoint {
    /// Before acquiring a connection slot or creating a native stream.
    ConnectionSlot = 1,
    /// Before reserving a receive frame slot.
    ReceiveSlot,
    /// Before reserving a transmit frame slot.
    SendSlot,
    /// Before reserving local backing bytes.
    LocalBytes,
    /// After local bytes, before global backing bytes.
    GlobalBytes,
    /// After byte reservations, before the fallible allocator call.
    Allocation,
    /// Supply a genuinely short destination to the production frame encoder.
    Encode,
    /// After encoding, before publishing a new admission slot.
    Admission,
}
#[derive(Default)]
pub(crate) struct Failure(AtomicU8);
impl Failure {
    pub(crate) fn arm(&self, point: FailurePoint) {
        self.0.store(point as u8, Ordering::SeqCst);
    }
    pub(crate) fn take(&self, point: FailurePoint) -> bool {
        self.0
            .compare_exchange(point as u8, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }
}
