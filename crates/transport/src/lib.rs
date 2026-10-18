//! Bounded AXF1 local transport, independent of engine, SQL, and native sockets.
//! See devdoc/contracts/comm-design.md. Local flush never means peer acknowledgement.
#![deny(missing_docs)]
mod connection;
mod driver;
mod resources;
mod sender;

pub use connection::{Connection, ConnectionPhase, ConnectionStats, ReceivedFrame};
pub use driver::{ConnectionId, Driver};
pub use resources::Transport;
pub use sender::{ConnectionSender, SendOutcome, SendTicket};

/// Maximum total wire bytes in a single AXF1 frame.
pub const MAX_WIRE_BYTES: usize =
    xadb_foundation::codec::MAX_FRAME_PAYLOAD + xadb_foundation::codec::FRAME_OVERHEAD;
/// Connection slots include terminal objects retained by application handles.
pub const MAX_CONNECTIONS: usize = 128;
/// Maximum number of listeners in one driver.
pub const MAX_LISTENERS: usize = 3;
/// Total charged payload backing capacity in one transport instance.
pub const PROCESS_PAYLOAD_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum readiness sleep and non-actionable readiness retry delay.
pub const WAIT_NANOS: u64 = 20_000_000;
const CONNECT_NANOS: u64 = 5_000_000_000;
const FRAME_NANOS: u64 = 30_000_000_000;
const IDLE_NANOS: u64 = 60_000_000_000;
const DRAIN_NANOS: u64 = 5_000_000_000;
const BYTE_BUDGET: usize = 65_536;
const FRAME_BUDGET: usize = 8;
const CALL_BUDGET: usize = 32;

/// Typed transport failure or nonterminal admission rejection; never SQL certainty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// Bad magic, length, checksum, or oversize payload.
    InvalidFrame,
    /// AXF1 version is unsupported.
    UnsupportedFormat,
    /// EOF inside a header/body/digest.
    TruncatedFrame,
    /// RX allocation or connection reservation failed.
    ResourceExhausted,
    /// Native I/O failed; inspect Error::io_kind.
    Io,
    /// A nonempty write returned zero.
    WriteZero,
    /// Connect completion was not observed within five seconds.
    ConnectTimedOut,
    /// Frame validation exceeded its absolute deadline.
    FrameTimedOut,
    /// Enqueued TX exceeded its absolute deadline.
    SendTimedOut,
    /// Open connection made no complete-frame progress in sixty seconds.
    IdleTimedOut,
    /// Accepted TX was not drained in five seconds.
    DrainTimedOut,
    /// Cancellation was observed.
    Cancelled,
    /// Owner closed or dropped, or admission is closed.
    Closed,
    /// Invalid adapter count, clock regression/overflow, or stale registration.
    AdapterViolation,
    /// No TX slot or bytes available; no admission took place.
    Backpressured,
    /// A shared lock was poisoned; this owner cannot resume service.
    Poisoned,
}

/// Error with original OS category. Connection identity/phase are available on
/// the owner; failed tickets additionally retain exact successful wire progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Error {
    /// Transport category.
    pub kind: ErrorKind,
    /// Original OS category, if there was an I/O error.
    pub io_kind: Option<std::io::ErrorKind>,
}
impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Self {
            kind,
            io_kind: None,
        }
    }
}
impl Error {
    pub(crate) fn io(error: std::io::Error) -> Self {
        Self {
            kind: ErrorKind::Io,
            io_kind: Some(error.kind()),
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "transport {:?} ({:?})", self.kind, self.io_kind)
    }
}
impl std::error::Error for Error {}
/// Result with transport-specific, nontransactional semantics.
pub type Result<T> = std::result::Result<T, Error>;

fn deadline(now: u64, duration: u64) -> Result<u64> {
    now.checked_add(duration)
        .ok_or(ErrorKind::AdapterViolation.into())
}
