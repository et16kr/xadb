use crate::{
    Error, ErrorKind, EventKind, FRAME_NANOS, Result, deadline,
    diagnostics::increment,
    resources::{Buffer, ClockDomain, Family},
};
use foundation::{
    codec::{FRAME_OVERHEAD, MAX_FRAME_PAYLOAD, encode_frame_into},
    runtime::{Cancellation, Reservation},
};
use std::sync::{Arc, Mutex};

/// Exactly one terminal local result. This does not certify peer receipt/execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendOutcome {
    /// Entire frame was accepted by local writes before its deadline.
    FlushedLocally,
    /// Connection stopped with this successful wire prefix, possibly the full frame.
    Failed {
        /// Terminal reason, retaining the OS category if applicable.
        error: Error,
        /// Successful wire bytes, including header/digest. Never inferred peer bytes.
        written_bytes: usize,
    },
}
pub(crate) struct Slot {
    _reservation: Reservation,
    pub(crate) sequence: u64,
    pub(crate) deadline: u64,
    pub(crate) queued: Option<Buffer>,
    pub(crate) result: Option<SendOutcome>,
    live_ticket: bool,
}
pub(crate) struct Admission {
    pub(crate) open: bool,
    pub(crate) fatal: Option<Error>,
    // Installed under the same gate when the owner enters a driver. Existing
    // sender handles then observe shutdown without waiting for the next step.
    pub(crate) driver_cancellation: Option<Cancellation>,
    next: u64,
    pub(crate) slots: [Option<Slot>; 4],
}
pub(crate) struct Shared {
    pub(crate) family: Arc<Family>,
    pub(crate) clock: Arc<ClockDomain>,
    pub(crate) cancellation: Cancellation,
    pub(crate) admission: Mutex<Admission>,
    pub(crate) registration: foundation::network::Registration,
}
impl Shared {
    pub(crate) fn new(
        family: Arc<Family>,
        clock: Arc<ClockDomain>,
        cancellation: Cancellation,
        open: bool,
        registration: foundation::network::Registration,
    ) -> Arc<Self> {
        Arc::new(Self {
            family,
            clock,
            cancellation,
            registration,
            admission: Mutex::new(Admission {
                open,
                fatal: None,
                driver_cancellation: None,
                next: 1,
                slots: std::array::from_fn(|_| None),
            }),
        })
    }
    pub(crate) fn check_cancellation(&self, admission: &Admission) -> Result<()> {
        if self.cancellation.check().is_err()
            || admission
                .driver_cancellation
                .as_ref()
                .is_some_and(|cancellation| cancellation.check().is_err())
        {
            return Err(ErrorKind::Cancelled.into());
        }
        Ok(())
    }
    fn check_admission(&self, admission: &Admission) -> Result<()> {
        self.check_cancellation(admission)?;
        if let Some(error) = admission.fatal {
            return Err(error);
        }
        if !admission.open {
            return Err(ErrorKind::Closed.into());
        }
        Ok(())
    }
    pub(crate) fn finish(&self, index: usize, outcome: SendOutcome) {
        let mut admission = self
            .admission
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(slot) = &mut admission.slots[index] {
            if slot.result.is_none() {
                slot.result = Some(outcome);
                slot.queued = None;
            }
            if !slot.live_ticket {
                admission.slots[index] = None;
            }
        }
    }
}

/// Cloneable producer handle; only TX admission is shared with the driver.
/// Old handles never attach to a new connection, even when native fd slots recycle.
#[derive(Clone)]
pub struct ConnectionSender(pub(crate) Arc<Shared>);
impl ConnectionSender {
    fn backpressure_error(&self) -> Error {
        // A retained terminal ticket can keep the slots full after close. Check
        // the gate at preparation failure so permanent errors stay terminal;
        // successful preparation still checks the gate at its enqueue point.
        let admission = match self.0.admission.lock() {
            Ok(admission) => admission,
            Err(_) => return ErrorKind::Poisoned.into(),
        };
        if let Err(error) = self.0.check_admission(&admission) {
            return error;
        }
        increment(&self.0.family.diagnostics.backpressured);
        ErrorKind::Backpressured.into()
    }
    /// Reserve, encode outside the gate, then atomically admit a FIFO frame.
    /// The input may be changed/dropped immediately after return.
    ///
    /// # Errors
    /// Backpressured is nonblocking rejection with no send effect. Closed/cancelled
    /// or invalid input also admits nothing. Preparation capacity failures report
    /// observed cancellation/terminal admission before Backpressured. A short
    /// mutex wait is possible.
    pub fn try_send(&self, payload: &[u8]) -> Result<SendTicket> {
        if payload.len() > MAX_FRAME_PAYLOAD {
            return Err(ErrorKind::InvalidFrame.into());
        }
        #[cfg(feature = "test-support")]
        if self
            .0
            .family
            .failure
            .take(crate::test_support::FailurePoint::SendSlot)
        {
            return Err(self.backpressure_error());
        }
        let slot_charge = self
            .0
            .family
            .tx_slots
            .reserve(1)
            .map_err(|_| self.backpressure_error())?;
        let mut buffer = Buffer::allocate(&self.0.family, false, payload.len() + FRAME_OVERHEAD)
            .map_err(|_| self.backpressure_error())?;
        #[cfg(feature = "test-support")]
        if self
            .0
            .family
            .failure
            .take(crate::test_support::FailurePoint::Encode)
        {
            let short = buffer.bytes.len() - 1;
            encode_frame_into(payload, &mut buffer.bytes[..short])
                .map_err(|_| ErrorKind::InvalidFrame)?;
        }
        encode_frame_into(payload, &mut buffer.bytes).map_err(|_| ErrorKind::InvalidFrame)?;
        let mut admission = self.0.admission.lock().map_err(|_| ErrorKind::Poisoned)?;
        #[cfg(feature = "test-support")]
        if self
            .0
            .family
            .failure
            .take(crate::test_support::FailurePoint::Admission)
        {
            return Err(ErrorKind::Closed.into());
        }
        self.0.check_admission(&admission)?;
        let expires = match self
            .0
            .clock
            .now()
            .and_then(|now| deadline(now, FRAME_NANOS))
        {
            Ok(expires) => expires,
            Err(error) => {
                admission.open = false;
                admission.fatal = Some(error);
                return Err(error);
            }
        };
        let next = admission.next.checked_add(1).ok_or_else(|| {
            admission.open = false;
            admission.fatal = Some(ErrorKind::AdapterViolation.into());
            Error::from(ErrorKind::AdapterViolation)
        })?;
        let index = admission
            .slots
            .iter()
            .position(Option::is_none)
            .ok_or(ErrorKind::Backpressured)?;
        let sequence = admission.next;
        admission.next = next;
        admission.slots[index] = Some(Slot {
            _reservation: slot_charge,
            sequence,
            deadline: expires,
            queued: Some(buffer),
            result: None,
            live_ticket: true,
        });
        let _ = self.0.family.copied_payload_bytes.fetch_update(
            std::sync::atomic::Ordering::Relaxed,
            std::sync::atomic::Ordering::Relaxed,
            |n| Some(n.saturating_add(payload.len() as u64)),
        );
        drop(admission);
        self.0.family.diagnostics.record(
            &self.0.clock,
            self.0.registration,
            EventKind::Enqueued,
            (payload.len() + FRAME_OVERHEAD) as u64,
            None,
        );
        Ok(SendTicket {
            shared: self.0.clone(),
            index,
            sequence,
        })
    }
    /// Request connection-wide cooperative cancellation; does not cancel one ticket.
    pub fn cancel(&self) {
        self.0.cancellation.cancel();
    }
}

/// One admitted send. Drop during progress does not cancel the send. A terminal
/// ticket retains its slot until consumed/dropped, bounding uncollected results.
pub struct SendTicket {
    shared: Arc<Shared>,
    index: usize,
    sequence: u64,
}
impl SendTicket {
    /// Local FIFO admission sequence, scoped to this connection.
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    /// Consume a terminal result, or return this same ticket if still pending.
    /// No stale ticket can observe a reused slot after successful collection.
    pub fn try_take(self) -> std::result::Result<SendOutcome, Self> {
        let result = {
            let admission = self
                .shared
                .admission
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            admission.slots[self.index]
                .as_ref()
                .filter(|s| s.sequence == self.sequence)
                .and_then(|s| s.result)
        };
        match result {
            Some(result) => Ok(result),
            None => Err(self),
        }
    }
}
impl Drop for SendTicket {
    fn drop(&mut self) {
        let mut admission = self
            .shared
            .admission
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(slot) = &mut admission.slots[self.index]
            && slot.sequence == self.sequence
        {
            slot.live_ticket = false;
            if slot.result.is_some() {
                admission.slots[self.index] = None;
            }
        }
    }
}
