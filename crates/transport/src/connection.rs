use crate::{
    BYTE_BUDGET, CALL_BUDGET, CONNECT_NANOS, DRAIN_NANOS, Error, ErrorKind, EventKind,
    FRAME_BUDGET, FRAME_NANOS, IDLE_NANOS, LatencyHistogram, MAX_WIRE_BYTES, Result, Transport,
    WAIT_NANOS, deadline,
    resources::{Buffer, Family},
    sender::{ConnectionSender, SendOutcome, Shared},
};
use foundation::{
    codec::{FRAME_HEADER_BYTES, FRAME_OVERHEAD, decode_frame, inspect_frame_header},
    network::{ByteStream, Connector, Interest, Ready, Registration},
    runtime::{Cancellation, Reservation},
};
use std::{
    io,
    sync::{Arc, atomic::Ordering},
};

/// Transport lifetime, independent of authentication or SQL session state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionPhase {
    /// Nonblocking connect is pending; payload admission is closed.
    Connecting,
    /// Frames may be admitted and received.
    Open,
    /// RX stopped and only accepted TX is being drained.
    Draining,
    /// Terminal; socket closed, but external leases/tickets remain valid.
    Closed,
}
/// Bounded per-connection counters; saturation is exposed by overflowed.
#[derive(Clone, Copy, Debug, Default)]
pub struct ConnectionStats {
    /// Successful read bytes, including framing.
    pub read_bytes: u64,
    /// Successful write bytes, including framing.
    pub written_bytes: u64,
    /// Frames validated and delivered into the bounded RX queue.
    pub received_frames: u64,
    /// Frames completed locally within their deadlines.
    pub flushed_frames: u64,
    /// Nonblocking attempts that returned WouldBlock.
    pub would_block: u64,
    /// Short successful reads/writes.
    pub short_io: u64,
    /// Total attempted reads/writes/connect completions.
    pub attempts: u64,
    /// At least one counter saturated.
    pub overflowed: bool,
    /// Charged RX/TX backing capacity, including externally held RX leases.
    pub queue_bytes: [u64; 2],
    /// Maximum charged RX/TX backing capacity sampled at reservation.
    pub peak_queue_bytes: [u64; 2],
    /// Reserved RX/TX slots, including preparation and terminal tickets.
    pub queue_slots: [u64; 2],
    /// Maximum reserved RX/TX slots sampled at reservation.
    pub peak_queue_slots: [u64; 2],
    /// Application payload bytes copied during accepted TX encoding.
    pub copied_payload_bytes: u64,
    /// Observed RX pause time in nanoseconds; sampled on owner control/progress.
    pub rx_paused_nanos: u64,
    /// Completed RX frame first-byte-to-validation latency.
    pub receive_latency: LatencyHistogram,
    /// Accepted TX enqueue-to-local-flush latency.
    pub send_latency: LatencyHistogram,
}
fn count(counter: &mut u64, amount: usize, overflow: &mut bool) {
    match counter.checked_add(amount as u64) {
        Some(n) => *counter = n,
        None => {
            *counter = u64::MAX;
            *overflow = true;
        }
    }
}
/// Validated frame with exclusive buffer ownership and byte/slot/connection leases.
/// Borrowed payload stays valid after connection close or driver Drop.
pub struct ReceivedFrame {
    buffer: Buffer,
    _slot: Reservation,
    _family: Arc<Family>,
}
impl ReceivedFrame {
    /// Opaque bytes. Validation happened before this frame became observable.
    pub fn payload(&self) -> &[u8] {
        &self.buffer.bytes
            [FRAME_HEADER_BYTES..self.buffer.bytes.len() - (FRAME_OVERHEAD - FRAME_HEADER_BYTES)]
    }
}
struct Receive {
    header: [u8; FRAME_HEADER_BYTES],
    filled: usize,
    body: Option<Buffer>,
    slot: Option<Reservation>,
    expires: Option<u64>,
}
impl Default for Receive {
    fn default() -> Self {
        Self {
            header: [0; FRAME_HEADER_BYTES],
            filled: 0,
            body: None,
            slot: None,
            expires: None,
        }
    }
}
struct Transmit {
    buffer: Buffer,
    index: usize,
    offset: usize,
    enqueued_at: u64,
}

/// Single driver owner. No socket I/O/checksum/allocation executes under admission
/// lock. Drop closes immediately and resolves every pending ticket without flush.
pub struct Connection {
    stream: Box<dyn ByteStream>,
    registration: Registration,
    pub(crate) shared: Arc<Shared>,
    phase: ConnectionPhase,
    terminal: Option<Error>,
    rx: Receive,
    received: [Option<ReceivedFrame>; 2],
    tx: Option<Transmit>,
    connect_expires: u64,
    idle_expires: u64,
    drain_expires: Option<u64>,
    retry_at: u64,
    peer_write_closed: bool,
    hangup_hint: bool,
    stats: ConnectionStats,
    pause_started: Option<u64>,
}
impl Transport {
    /// Adopt a caller-owned, already connected stream into a reserved family.
    /// External creation of this stream is outside transport listener admission;
    /// Driver reserves before accepting native connections.
    ///
    /// # Errors
    /// Slot exhaustion, invalid registration, or invalid clock prevents adoption.
    pub fn accept(&self, stream: Box<dyn ByteStream>) -> Result<Connection> {
        let family = self.reserve_connection()?;
        Connection::new(self, stream, family, false, self.clock.now()?)
    }
    /// Reserve a slot before making a connect effect; timestamp precedes the call.
    ///
    /// # Errors
    /// No native connect occurs on resource/clock failure. I/O errors are preserved.
    pub fn connect(&self, connector: &mut dyn Connector) -> Result<Connection> {
        let family = self.reserve_connection()?;
        let now = self.clock.now()?;
        deadline(now, IDLE_NANOS)?;
        let stream = connector.connect().map_err(Error::io)?;
        Connection::new(self, stream, family, true, now)
    }
}
impl Connection {
    pub(crate) fn new(
        transport: &Transport,
        stream: Box<dyn ByteStream>,
        family: Arc<Family>,
        connecting: bool,
        now: u64,
    ) -> Result<Self> {
        let registration = stream.registration();
        if registration.generation == 0 {
            return Err(ErrorKind::AdapterViolation.into());
        }
        let shared = Shared::new(
            family,
            transport.clock.clone(),
            Cancellation::default(),
            !connecting,
            registration,
        );
        Ok(Self {
            stream,
            registration,
            shared,
            phase: if connecting {
                ConnectionPhase::Connecting
            } else {
                ConnectionPhase::Open
            },
            terminal: None,
            rx: Receive::default(),
            received: [None, None],
            tx: None,
            connect_expires: deadline(now, CONNECT_NANOS)?,
            idle_expires: deadline(now, IDLE_NANOS)?,
            drain_expires: None,
            retry_at: 0,
            peer_write_closed: false,
            hangup_hint: false,
            stats: ConnectionStats::default(),
            pause_started: None,
        })
    }
    /// Obtain a producer handle; Connecting still rejects payloads until opened.
    pub fn sender(&self) -> ConnectionSender {
        ConnectionSender(self.shared.clone())
    }
    /// Cancellation token shared with producer admission and progress checks.
    pub fn cancellation(&self) -> Cancellation {
        self.shared.cancellation.clone()
    }
    /// Current connection lifetime phase.
    pub fn phase(&self) -> ConnectionPhase {
        self.phase
    }
    /// First terminal reason, immutable after closure.
    pub fn terminal_error(&self) -> Option<Error> {
        self.terminal
    }
    /// Current bounded counters.
    pub fn stats(&self) -> ConnectionStats {
        let family = &self.shared.family;
        let mut stats = self.stats;
        stats.queue_bytes = [family.rx.used(), family.tx.used()];
        stats.queue_slots = [family.rx_slots.used(), family.tx_slots.used()];
        stats.peak_queue_bytes = [family.rx.peak(), family.tx.peak()];
        stats.peak_queue_slots = [family.rx_slots.peak(), family.tx_slots.peak()];
        stats.copied_payload_bytes = family.copied_payload_bytes.load(Ordering::Relaxed);
        stats.overflowed |= stats.copied_payload_bytes == u64::MAX;
        stats
    }
    /// Exact opaque stream incarnation.
    pub fn registration(&self) -> Registration {
        self.registration
    }
    /// Whether a boundary EOF was observed; HUP alone never sets this flag.
    pub fn peer_write_closed(&self) -> bool {
        self.peer_write_closed
    }
    /// Transfer the oldest verified frame; byte and slot charges remain attached.
    pub fn take_received(&mut self) -> Option<ReceivedFrame> {
        let result = self.received[0].take();
        self.received[0] = self.received[1].take();
        result
    }
    fn can_read(&self) -> bool {
        self.phase == ConnectionPhase::Open
            && (self.rx.filled != 0
                || (self.shared.family.rx_slots.used() < 2
                    && self.shared.family.rx.used() <= MAX_WIRE_BYTES as u64))
    }
    fn has_tx(&self) -> bool {
        self.tx.is_some()
            || self
                .shared
                .admission
                .lock()
                .map(|a| a.slots.iter().flatten().any(|s| s.result.is_none()))
                .unwrap_or(true)
    }
    /// State-dependent interest; paused RX and empty TX are actually omitted.
    pub fn interest(&self) -> Option<Interest> {
        let now = self.shared.clock.now().ok()?;
        if self.phase == ConnectionPhase::Closed || now < self.retry_at {
            return None;
        }
        let readable = self.can_read();
        let writable = self.phase == ConnectionPhase::Connecting || self.has_tx();
        (readable || writable).then_some(Interest {
            registration: self.registration,
            readable,
            writable,
        })
    }
    /// Stop admission and partial RX. Verified RX remains collectible.
    /// Repeated drain does not extend its deadline; Connecting drain closes.
    pub fn begin_drain(&mut self) {
        if matches!(
            self.phase,
            ConnectionPhase::Draining | ConnectionPhase::Closed
        ) {
            return;
        }
        if self.phase == ConnectionPhase::Connecting {
            self.close(ErrorKind::Closed.into());
            return;
        }
        // The drain deadline begins at the same gate closure that excludes
        // concurrent producers, not before waiting to acquire that gate.
        let expires = (|| -> Result<u64> {
            let mut admission = self
                .shared
                .admission
                .lock()
                .map_err(|_| ErrorKind::Poisoned)?;
            admission.open = false;
            self.shared
                .clock
                .now()
                .and_then(|now| deadline(now, DRAIN_NANOS))
        })();
        match expires {
            Ok(expires) => self.drain_expires = Some(expires),
            Err(error) => {
                self.close(error);
                return;
            }
        }
        self.phase = ConnectionPhase::Draining;
        self.rx = Receive::default();
        if !self.has_tx() {
            self.close(ErrorKind::Closed.into());
        }
    }
    /// Observe local connection cancellation immediately and close without drain.
    pub fn cancel(&mut self) {
        self.shared.cancellation.cancel();
        self.close(ErrorKind::Cancelled.into());
    }
    pub(crate) fn close(&mut self, error: Error) {
        if self.phase == ConnectionPhase::Closed {
            return;
        }
        {
            let mut a = self
                .shared
                .admission
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            a.open = false;
            a.fatal = Some(error);
        }
        self.phase = ConnectionPhase::Closed;
        self.terminal = Some(error);
        self.shared.family.diagnostics.record(
            &self.shared.clock,
            self.registration,
            EventKind::Closed,
            0,
            Some(error.kind),
        );
        self.stream.close();
        self.rx = Receive::default();
        if let Some(tx) = self.tx.take() {
            self.shared.finish(
                tx.index,
                SendOutcome::Failed {
                    error,
                    written_bytes: tx.offset,
                },
            );
        }
        for index in 0..4 {
            self.shared.finish(
                index,
                SendOutcome::Failed {
                    error,
                    written_bytes: 0,
                },
            );
        }
    }
    fn check(&mut self) -> Result<u64> {
        let now = self.shared.clock.now()?;
        if let Some(started) = self.pause_started.take() {
            count(
                &mut self.stats.rx_paused_nanos,
                (now - started) as usize,
                &mut self.stats.overflowed,
            );
        }
        if self.phase == ConnectionPhase::Open && !self.can_read() {
            self.pause_started = Some(now);
        }
        if self.shared.cancellation.check().is_err() {
            return Err(ErrorKind::Cancelled.into());
        }
        let a = self
            .shared
            .admission
            .lock()
            .map_err(|_| ErrorKind::Poisoned)?;
        self.shared.check_cancellation(&a)?;
        if let Some(error) = a.fatal {
            return Err(error);
        }
        if self.phase == ConnectionPhase::Connecting && now >= self.connect_expires {
            return Err(ErrorKind::ConnectTimedOut.into());
        }
        if self.drain_expires.is_some_and(|t| now >= t) {
            return Err(ErrorKind::DrainTimedOut.into());
        }
        if self.rx.expires.is_some_and(|t| now >= t) {
            return Err(ErrorKind::FrameTimedOut.into());
        }
        if a.slots
            .iter()
            .flatten()
            .any(|s| s.result.is_none() && now >= s.deadline)
        {
            return Err(ErrorKind::SendTimedOut.into());
        }
        if self.phase == ConnectionPhase::Open && now >= self.idle_expires {
            return Err(ErrorKind::IdleTimedOut.into());
        }
        Ok(now)
    }
    pub(crate) fn poll_control(&mut self) {
        if self.phase == ConnectionPhase::Closed {
            return;
        }
        if let Err(error) = self.check() {
            self.close(error);
        } else if self.phase == ConnectionPhase::Draining && !self.has_tx() {
            self.close(ErrorKind::Closed.into());
        }
    }
    /// Progress both directions within 64 KiB/8-frame/32-attempt budgets each.
    /// Stale events are ignored, deadlines/cancellation still advance without I/O.
    /// Terminal errors are stored on the owner and pending tickets exactly once.
    pub fn advance(&mut self, registration: Registration, ready: Ready) {
        if self.phase == ConnectionPhase::Closed {
            return;
        }
        let result = self.progress(registration, ready);
        if let Err(error) = result {
            self.close(error);
        }
    }
    fn progress(&mut self, registration: Registration, ready: Ready) -> Result<()> {
        let now = self.check()?;
        if registration != self.registration || now < self.retry_at {
            return Ok(());
        }
        self.hangup_hint |= ready.hangup;
        if ready.invalid {
            return Err(ErrorKind::AdapterViolation.into());
        }
        if ready.error {
            let error = self.stream.take_error().map_err(Error::io)?;
            self.check()?;
            if let Some(error) = error {
                return Err(Error::io(error));
            }
        }
        if self.phase == ConnectionPhase::Connecting {
            if ready.writable || ready.error || ready.hangup {
                self.check()?;
                count(&mut self.stats.attempts, 1, &mut self.stats.overflowed);
                let result = self.stream.finish_connect();
                let now = self.check()?;
                match result {
                    Ok(()) => {
                        self.idle_expires = deadline(now, IDLE_NANOS)?;
                        self.shared
                            .admission
                            .lock()
                            .map_err(|_| ErrorKind::Poisoned)?
                            .open = true;
                        self.phase = ConnectionPhase::Open;
                    }
                    Err(e)
                        if matches!(
                            e.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                        ) =>
                    {
                        self.retry_at = deadline(now, WAIT_NANOS)?
                    }
                    Err(e) => return Err(Error::io(e)),
                }
            }
            return Ok(());
        }
        if ready.error && !ready.readable && !ready.writable && !self.hangup_hint {
            self.retry_at = deadline(self.check()?, WAIT_NANOS)?;
            return Ok(());
        }
        if ready.readable || self.hangup_hint {
            self.receive(true)?;
        }
        if self.phase != ConnectionPhase::Closed && (ready.writable || ready.hangup) {
            self.transmit(true)?;
        }
        self.check()?;
        if self.phase == ConnectionPhase::Draining && !self.has_tx() {
            self.close(ErrorKind::Closed.into());
        }
        Ok(())
    }
    fn receive(&mut self, hint: bool) -> Result<()> {
        let mut bytes = 0;
        let mut frames = 0;
        for _ in 0..CALL_BUDGET {
            if !self.can_read() || bytes >= BYTE_BUDGET || frames >= FRAME_BUDGET {
                break;
            }
            let before = self.check()?;
            let target = if let Some(body) = &mut self.rx.body {
                &mut body.bytes[self.rx.filled..]
            } else {
                &mut self.rx.header[self.rx.filled..]
            };
            let requested = target.len().min(BYTE_BUDGET - bytes);
            let result = self.stream.try_read(&mut target[..requested]);
            count(&mut self.stats.attempts, 1, &mut self.stats.overflowed);
            // Account successful progress before detecting a deadline crossed in I/O.
            if let Ok(n) = result {
                if n > requested {
                    return Err(ErrorKind::AdapterViolation.into());
                }
                count(&mut self.stats.read_bytes, n, &mut self.stats.overflowed);
                if n > 0 && n < requested {
                    count(&mut self.stats.short_io, 1, &mut self.stats.overflowed);
                }
                if n > 0 && self.rx.filled == 0 {
                    self.rx.expires = Some(deadline(before, FRAME_NANOS)?);
                }
            }
            let now = self.check()?;
            match result {
                Ok(0) => {
                    if self.rx.filled != 0 {
                        return Err(ErrorKind::TruncatedFrame.into());
                    }
                    self.peer_write_closed = true;
                    self.begin_drain();
                    return Ok(());
                }
                Ok(n) => {
                    self.rx.filled += n;
                    bytes += n;
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    count(&mut self.stats.would_block, 1, &mut self.stats.overflowed);
                    if hint {
                        self.retry_at = deadline(now, WAIT_NANOS)?;
                    }
                    break;
                }
                Err(e) => return Err(Error::io(e)),
            }
            if self.rx.body.is_none() && self.rx.filled == FRAME_HEADER_BYTES {
                let size = inspect_frame_header(&self.rx.header).map_err(codec_error)?;
                #[cfg(feature = "test-support")]
                if self
                    .shared
                    .family
                    .failure
                    .take(crate::test_support::FailurePoint::ReceiveSlot)
                {
                    return Err(ErrorKind::ResourceExhausted.into());
                }
                self.rx.slot = Some(
                    self.shared
                        .family
                        .rx_slots
                        .reserve(1)
                        .map_err(|_| ErrorKind::ResourceExhausted)?,
                );
                let mut body = Buffer::allocate(&self.shared.family, true, size)?;
                body.bytes[..FRAME_HEADER_BYTES].copy_from_slice(&self.rx.header);
                self.rx.body = Some(body);
                self.check()?;
            }
            if self
                .rx
                .body
                .as_ref()
                .is_some_and(|b| self.rx.filled == b.bytes.len())
            {
                let body = self.rx.body.take().ok_or(ErrorKind::AdapterViolation)?;
                decode_frame(&body.bytes).map_err(codec_error)?;
                let now = self.check()?;
                let started = self.rx.expires.ok_or(ErrorKind::AdapterViolation)? - FRAME_NANOS;
                self.stats.receive_latency.observe(now - started);
                self.shared.family.diagnostics.record(
                    &self.shared.clock,
                    self.registration,
                    EventKind::Received,
                    body.bytes.len() as u64,
                    None,
                );
                let slot = self.rx.slot.take().ok_or(ErrorKind::AdapterViolation)?;
                let index = self
                    .received
                    .iter()
                    .position(Option::is_none)
                    .ok_or(ErrorKind::AdapterViolation)?;
                self.received[index] = Some(ReceivedFrame {
                    buffer: body,
                    _slot: slot,
                    _family: self.shared.family.clone(),
                });
                self.rx = Receive::default();
                self.idle_expires = deadline(now, IDLE_NANOS)?;
                frames += 1;
                count(
                    &mut self.stats.received_frames,
                    1,
                    &mut self.stats.overflowed,
                );
            }
        }
        Ok(())
    }
    fn transmit(&mut self, hint: bool) -> Result<()> {
        let mut bytes = 0;
        let mut frames = 0;
        for _ in 0..CALL_BUDGET {
            if bytes >= BYTE_BUDGET || frames >= FRAME_BUDGET {
                break;
            }
            self.check()?;
            if self.tx.is_none() {
                let mut admission = self
                    .shared
                    .admission
                    .lock()
                    .map_err(|_| ErrorKind::Poisoned)?;
                let index = admission
                    .slots
                    .iter()
                    .enumerate()
                    .filter_map(|(i, s)| {
                        s.as_ref()
                            .filter(|s| s.queued.is_some())
                            .map(|s| (i, s.sequence))
                    })
                    .min_by_key(|(_, sequence)| *sequence)
                    .map(|(i, _)| i);
                if let Some(index) = index {
                    let buffer = admission.slots[index]
                        .as_mut()
                        .and_then(|s| s.queued.take())
                        .ok_or(ErrorKind::AdapterViolation)?;
                    let enqueued_at = admission.slots[index]
                        .as_ref()
                        .ok_or(ErrorKind::AdapterViolation)?
                        .deadline
                        - FRAME_NANOS;
                    self.tx = Some(Transmit {
                        buffer,
                        index,
                        offset: 0,
                        enqueued_at,
                    });
                } else {
                    break;
                }
            }
            self.check()?;
            let tx = self.tx.as_mut().ok_or(ErrorKind::AdapterViolation)?;
            let requested = (tx.buffer.bytes.len() - tx.offset).min(BYTE_BUDGET - bytes);
            let result = self
                .stream
                .try_write(&tx.buffer.bytes[tx.offset..tx.offset + requested]);
            count(&mut self.stats.attempts, 1, &mut self.stats.overflowed);
            if let Ok(n) = result {
                if n > requested {
                    return Err(ErrorKind::AdapterViolation.into());
                }
                tx.offset += n;
                bytes += n;
                count(&mut self.stats.written_bytes, n, &mut self.stats.overflowed);
                if n > 0 && n < requested {
                    count(&mut self.stats.short_io, 1, &mut self.stats.overflowed);
                }
            }
            let now = self.check()?;
            match result {
                Ok(0) => return Err(ErrorKind::WriteZero.into()),
                Ok(_) => (),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    count(&mut self.stats.would_block, 1, &mut self.stats.overflowed);
                    if hint {
                        self.retry_at = deadline(now, WAIT_NANOS)?;
                    }
                    break;
                }
                Err(e) => return Err(Error::io(e)),
            }
            if self
                .tx
                .as_ref()
                .is_some_and(|t| t.offset == t.buffer.bytes.len())
            {
                let tx = self.tx.take().ok_or(ErrorKind::AdapterViolation)?;
                let elapsed = now - tx.enqueued_at;
                self.stats.send_latency.observe(elapsed);
                self.shared.family.diagnostics.record(
                    &self.shared.clock,
                    self.registration,
                    EventKind::Flushed,
                    elapsed,
                    None,
                );
                self.shared.finish(tx.index, SendOutcome::FlushedLocally);
                self.idle_expires = deadline(now, IDLE_NANOS)?;
                frames += 1;
                count(
                    &mut self.stats.flushed_frames,
                    1,
                    &mut self.stats.overflowed,
                );
            }
        }
        Ok(())
    }
    pub(crate) fn reclaimable(&self) -> bool {
        self.phase == ConnectionPhase::Closed
            && self.received.iter().all(Option::is_none)
            && Arc::strong_count(&self.shared) == 1
            && Arc::strong_count(&self.shared.family) == 1
    }
    pub(crate) fn next_deadline(&self) -> Result<u64> {
        let now = self.shared.clock.now()?;
        let mut next = deadline(now, WAIT_NANOS)?;
        if self.phase == ConnectionPhase::Connecting {
            next = next.min(self.connect_expires);
        }
        if self.phase == ConnectionPhase::Open {
            next = next.min(self.idle_expires);
        }
        if let Some(t) = self.rx.expires {
            next = next.min(t);
        }
        if let Some(t) = self.drain_expires {
            next = next.min(t);
        }
        let a = self
            .shared
            .admission
            .lock()
            .map_err(|_| ErrorKind::Poisoned)?;
        for s in a.slots.iter().flatten().filter(|s| s.result.is_none()) {
            next = next.min(s.deadline);
        }
        if now < self.retry_at {
            next = next.min(self.retry_at);
        }
        Ok(next)
    }
}
fn codec_error(error: foundation::Error) -> Error {
    if error.code == foundation::ErrorCode::UnsupportedFormat {
        ErrorKind::UnsupportedFormat.into()
    } else {
        ErrorKind::InvalidFrame.into()
    }
}
impl Drop for Connection {
    fn drop(&mut self) {
        self.close(ErrorKind::Closed.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use foundation::runtime::Clock;
    use std::sync::{Mutex, Weak};

    #[derive(Default)]
    struct GateClock(Mutex<Option<Weak<Shared>>>);
    impl Clock for GateClock {
        fn now_nanos(&self) -> u64 {
            if let Some(shared) = self.0.lock().unwrap().as_ref().and_then(Weak::upgrade) {
                // This fixture enables the assertion only for begin_drain's read.
                assert!(
                    matches!(
                        shared.admission.try_lock(),
                        Err(std::sync::TryLockError::WouldBlock)
                    ),
                    "drain timestamp sampled before admission gate closure"
                );
            }
            0
        }
    }
    struct EmptyStream;
    impl ByteStream for EmptyStream {
        fn registration(&self) -> Registration {
            Registration {
                slot: 0,
                generation: 1,
            }
        }
        fn try_read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::ErrorKind::WouldBlock.into())
        }
        fn try_write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::WouldBlock.into())
        }
        fn finish_connect(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn take_error(&mut self) -> io::Result<Option<io::Error>> {
            Ok(None)
        }
        fn close(&mut self) {}
    }
    #[test]
    fn drain_timestamp_is_sampled_under_the_admission_gate() {
        let clock = Arc::new(GateClock::default());
        let transport = Transport::new(clock.clone());
        let mut connection = transport.accept(Box::new(EmptyStream)).unwrap();
        let ticket = connection.sender().try_send(&[]).unwrap();
        *clock.0.lock().unwrap() = Some(Arc::downgrade(&connection.shared));
        connection.begin_drain();
        assert_eq!(connection.phase(), ConnectionPhase::Draining);
        assert_eq!(connection.drain_expires, Some(DRAIN_NANOS));
        *clock.0.lock().unwrap() = None;
        drop(connection);
        assert!(matches!(ticket.try_take(), Ok(SendOutcome::Failed { .. })));
    }
}
