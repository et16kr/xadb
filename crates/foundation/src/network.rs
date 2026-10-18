//! Nonblocking byte effects. No SQL, transport framing, native fd, or OS flags.
use std::{io, time::Duration};

/// Opaque registration identity. Adapters must never recycle a generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Registration {
    /// Bounded adapter slot; not an operating-system descriptor.
    pub slot: u32,
    /// Nonzero incarnation of this slot.
    pub generation: u64,
}

/// Requested readiness. Empty interests must be omitted from wait sets.
#[derive(Clone, Copy, Debug)]
pub struct Interest {
    /// Identity whose events the owner will validate.
    pub registration: Registration,
    /// Read or accept may make progress.
    pub readable: bool,
    /// Write or pending connect may make progress.
    pub writable: bool,
}

/// Readiness hints, never proof of EOF, successful connect, or successful I/O.
#[derive(Clone, Copy, Debug, Default)]
pub struct Ready {
    /// Bytes or an accept may be available.
    pub readable: bool,
    /// Write or connect completion may be available.
    pub writable: bool,
    /// Read buffered bytes before interpreting EOF.
    pub hangup: bool,
    /// Owner must inspect the actual socket error.
    pub error: bool,
    /// Invalid registration is terminal, not a reusable fd event.
    pub invalid: bool,
}

/// One event correlated to an adapter registration generation.
#[derive(Clone, Copy, Debug)]
pub struct ReadinessEvent {
    /// Exact identity from the wait request.
    pub registration: Registration,
    /// Observed hints.
    pub ready: Ready,
}

/// Single-owner nonblocking stream. Errors contain no hidden successful prefix.
/// Calls may return Interrupted/WouldBlock. Successful counts are at most the
/// supplied slice length. Drop/close release handles without flushing or draining.
pub trait ByteStream: Send {
    /// Stable for this stream's lifetime, never a raw fd.
    fn registration(&self) -> Registration;
    /// Read a successful prefix; zero on a nonempty buffer means EOF.
    fn try_read(&mut self, output: &mut [u8]) -> io::Result<usize>;
    /// Write a successful prefix; zero on a nonempty buffer is a terminal failure.
    fn try_write(&mut self, input: &[u8]) -> io::Result<usize>;
    /// Verify connect completion using the actual socket error and peer state.
    /// WouldBlock means pending, even if readiness claimed writable.
    fn finish_connect(&mut self) -> io::Result<()>;
    /// Inspect and clear a pending socket error; None does not mean EOF.
    fn take_error(&mut self) -> io::Result<Option<io::Error>>;
    /// Idempotently unregister and release the owned native handle. No flush.
    fn close(&mut self);
}

/// Adapter configured with a validated local endpoint. No DNS or remote fallback.
pub trait Connector {
    /// Start nonblocking connect. Caller reserves its connection slot beforehand.
    fn connect(&mut self) -> io::Result<Box<dyn ByteStream>>;
}

/// Listener whose pathname/address ownership is enforced by its adapter.
pub trait Listener: Send {
    /// Stable opaque identity.
    fn registration(&self) -> Registration;
    /// Make at most one accept attempt after caller reserves a connection slot.
    fn try_accept(&mut self) -> io::Result<Box<dyn ByteStream>>;
    /// Stop admission, unregister, and release only owned endpoint resources.
    fn close(&mut self);
}

/// Level-triggered bounded readiness effect. No connection/lifecycle lock may be
/// held by the caller. Empty lists still wait. Output storage is caller-owned;
/// return at most its length, with no lost events when it covers all interests.
pub trait ReadinessWait {
    /// Wait no longer than timeout (apart from OS scheduling); Interrupted may
    /// return early. Stale events are filtered again by the connection owner.
    fn wait(
        &mut self,
        interests: &[Interest],
        timeout: Duration,
        output: &mut [ReadinessEvent],
    ) -> io::Result<usize>;
}
