//! OS-independent positional I/O contracts and shared retry/error handling. Error after a write/resize/sync is conservatively uncertain.
//! Actual file/fault composition is tested in xadb-platform `tests/contracts.rs`.
//! Caller owns file paths, publication ordering, writer exclusion and recovery policy.
use crate::{Error, ErrorCode, Result};
use std::io;

/// Positional file effects supplied by the composition layer.
/// Reads/writes may be short; failures after a mutating effect can be uncertain.
/// Implementations must preserve explicit offsets and must not claim successful
/// sync when the platform cannot provide the declared durability operation.
pub trait FileIo {
    /// Read at an explicit offset; success may fill only a prefix.
    ///
    /// # Errors
    /// Return OS read failures. Callers must discard partially filled output on error.
    fn read_at(&mut self, bytes: &mut [u8], offset: u64) -> io::Result<usize>;
    /// Write a prefix at the explicit offset; never silently use append semantics.
    ///
    /// # Errors
    /// Return OS failures even when a prefix may already have been written.
    fn write_at(&mut self, bytes: &[u8], offset: u64) -> io::Result<usize>;
    /// Request persistence of this file's contents and metadata.
    /// Does not publish a namespace change or make a DB transaction durable.
    ///
    /// # Errors
    /// Failure is uncertain and must not be treated as proof that earlier writes vanished.
    fn sync_all(&mut self) -> io::Result<()>;
    /// Resize the file; caller owns publication ordering and exclusive mutation.
    ///
    /// # Errors
    /// Return OS errors without promising that the old size was preserved.
    fn set_len(&mut self, len: u64) -> io::Result<()>;
}

fn range(offset: u64, len: usize) -> Result<()> {
    // XADB limits file offsets to signed 64-bit on every platform.
    // Reject overflow before the first effect.
    if offset
        .checked_add(len as u64)
        .is_none_or(|end| end > i64::MAX as u64)
    {
        return Err(Error::new(
            ErrorCode::LimitExceeded,
            "file offset outside signed 64-bit range",
        ));
    }
    Ok(())
}

/// Write all bytes, retrying Interrupted and advancing over successful short writes.
/// The caller owns writer exclusion and must not change shared handle flags concurrently.
/// This can block in the adapter, performs no sync, and does not consult cancellation.
///
/// # Errors
/// Out-of-range signed-64-bit offsets fail before any effect. Zero writes, invalid counts,
/// and adapter errors return [`ErrorCode::IoWriteUnknown`]; a prefix may remain and
/// the caller must resolve the outcome instead of automatically replaying the operation.
pub fn write_all_at(file: &mut impl FileIo, bytes: &[u8], offset: u64) -> Result<()> {
    range(offset, bytes.len())?;
    let mut done = 0;
    while done < bytes.len() {
        match file.write_at(&bytes[done..], offset + done as u64) {
            Ok(0) => {
                return Err(Error::io(
                    ErrorCode::IoWriteUnknown,
                    "zero-length write",
                    io::ErrorKind::WriteZero.into(),
                ));
            }
            Ok(n) if n <= bytes.len() - done => done += n,
            Ok(_) => {
                return Err(Error::new(
                    ErrorCode::IoWriteUnknown,
                    "invalid write count from adapter",
                ));
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => {
                return Err(Error::io(
                    ErrorCode::IoWriteUnknown,
                    "write outcome requires caller resolution",
                    e,
                ));
            }
        }
    }
    Ok(())
}

/// On read error the output buffer may contain a prefix; caller must discard it.
/// Retries Interrupted and short reads; may block in the adapter.
///
/// # Errors
/// Out-of-range offsets fail before reading. EOF, invalid counts, and adapter errors
/// return [`ErrorCode::IoRead`], with persistent state unchanged and output invalid.
pub fn read_exact_at(file: &mut impl FileIo, bytes: &mut [u8], offset: u64) -> Result<()> {
    range(offset, bytes.len())?;
    let mut done = 0;
    while done < bytes.len() {
        match file.read_at(&mut bytes[done..], offset + done as u64) {
            Ok(0) => {
                return Err(Error::io(
                    ErrorCode::IoRead,
                    "unexpected EOF",
                    io::ErrorKind::UnexpectedEof.into(),
                ));
            }
            Ok(n) if n <= bytes.len() - done => done += n,
            Ok(_) => {
                return Err(Error::new(
                    ErrorCode::IoRead,
                    "invalid read count from adapter",
                ));
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(Error::io(ErrorCode::IoRead, "read failed", e)),
        }
    }
    Ok(())
}

/// Sync the supplied file, preserving uncertainty on failure. May block in the adapter.
/// Namespace publication/directory sync and DB durability belong to the caller.
///
/// # Errors
/// Returns [`ErrorCode::IoWriteUnknown`] on adapter failure; no automatic retry.
pub fn sync(file: &mut impl FileIo) -> Result<()> {
    file.sync_all()
        .map_err(|e| Error::io(ErrorCode::IoWriteUnknown, "sync outcome unknown", e))
}

/// Resize within the signed-64-bit range; does not sync or publish the result.
///
/// # Errors
/// Invalid length fails before effects. Adapter errors return [`ErrorCode::IoWriteUnknown`].
pub fn resize(file: &mut impl FileIo, len: u64) -> Result<()> {
    range(len, 0)?;
    file.set_len(len)
        .map_err(|e| Error::io(ErrorCode::IoWriteUnknown, "resize outcome unknown", e))
}

#[cfg(feature = "test-support")]
/// Test-only fault injection with explicit receipts and mandatory completion checking.
pub mod fault {
    use super::{Error, ErrorCode, FileIo, Result, io};

    /// One fault action on one specified FileIo call.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Action {
        /// Limit a real write to a nonempty, strictly shorter prefix.
        ShortWrite(usize),
        /// Perform a real nonempty prefix write, then report an error.
        WriteThenError(usize),
        /// Report an error only after a successful real sync.
        SyncErrorAfterFlush,
        /// Fail a read before delegating to the underlying file.
        ReadError,
        /// Return zero for a nonempty write.
        ZeroWrite,
    }

    /// Observed call details; planned injection is distinct from an applied effect.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Receipt {
        /// One-based call number.
        pub call: usize,
        /// File operation name.
        pub operation: &'static str,
        /// Action selected for this call, if any.
        pub scheduled: Option<Action>,
        /// Action actually applied; None if wrong operation or underlying failure prevented it.
        pub injected: Option<Action>,
        /// Observed successful byte count, including a prefix before an injected error.
        pub completed_bytes: Option<usize>,
        /// Returned I/O error kind, when present.
        pub error: Option<io::ErrorKind>,
    }

    /// One selected fault, on a 1-based FileIo call. Delegates other calls to real I/O.
    /// Receipt buffer is bounded; dropped receipts are counted, never silently unlimited.
    pub struct FaultFile<F> {
        /// Underlying owned adapter. Direct calls bypass fault accounting and cannot qualify injection.
        pub inner: F,
        call: usize,
        fault_call: usize,
        action: Action,
        receipts: Vec<Receipt>,
        dropped_receipts: usize,
        fault_applied: bool,
        fault_selected: bool,
    }
    impl<F: FileIo> FaultFile<F> {
        /// Take ownership of an adapter and reserve 256 receipts before any I/O.
        ///
        /// # Errors
        /// Returns [`ErrorCode::InvalidSchedule`] for call zero or
        /// [`ErrorCode::QuotaExceeded`] when receipt allocation fails.
        pub fn new(inner: F, fault_call: usize, action: Action) -> Result<Self> {
            if fault_call == 0 {
                return Err(Error::new(
                    ErrorCode::InvalidSchedule,
                    "fault call is 1-based",
                ));
            }
            let mut receipts = Vec::new();
            receipts
                .try_reserve_exact(256)
                .map_err(|_| Error::new(ErrorCode::QuotaExceeded, "receipt buffer"))?;
            Ok(Self {
                inner,
                call: 0,
                fault_call,
                action,
                receipts,
                dropped_receipts: 0,
                fault_applied: false,
                fault_selected: false,
            })
        }
        /// Borrow retained receipts until the next mutable use of this adapter.
        pub fn receipts(&self) -> &[Receipt] {
            &self.receipts
        }
        /// Count receipts lost beyond the bounded capacity; saturates at usize::MAX.
        pub fn dropped_receipts(&self) -> usize {
            self.dropped_receipts
        }
        /// Required at the end of a fault scenario; absence of the requested fault is not success.
        /// Does not sync, close, or change the underlying file.
        ///
        /// # Errors
        /// Returns [`ErrorCode::InvalidSchedule`] if the action was not applied or
        /// receipts were lost. Also inspect returned I/O errors and actual file bytes.
        pub fn finish(&self) -> Result<()> {
            if self.fault_applied && self.dropped_receipts == 0 {
                Ok(())
            } else {
                Err(Error::new(
                    ErrorCode::InvalidSchedule,
                    "fault not applied or receipts lost",
                ))
            }
        }
        fn next(&mut self) -> Option<Action> {
            self.call = self.call.saturating_add(1);
            if !self.fault_selected && self.call == self.fault_call {
                self.fault_selected = true;
                Some(self.action)
            } else {
                None
            }
        }
        fn record(
            &mut self,
            operation: &'static str,
            scheduled: Option<Action>,
            applied: bool,
            n: Option<usize>,
            error: Option<io::ErrorKind>,
        ) {
            self.fault_applied |= applied;
            if self.receipts.len() < 256 {
                self.receipts.push(Receipt {
                    call: self.call,
                    operation,
                    scheduled,
                    injected: scheduled.filter(|_| applied),
                    completed_bytes: n,
                    error,
                });
            } else {
                self.dropped_receipts = self.dropped_receipts.saturating_add(1);
            }
        }
    }
    fn failure() -> io::Error {
        io::Error::other("injected I/O failure")
    }
    fn wrong_operation() -> io::Error {
        io::Error::new(io::ErrorKind::InvalidInput, "fault operation mismatch")
    }
    impl<F: FileIo> FileIo for FaultFile<F> {
        fn read_at(&mut self, bytes: &mut [u8], offset: u64) -> io::Result<usize> {
            let action = self.next();
            let result = match action {
                Some(Action::ReadError) => Err(failure()),
                Some(_) => Err(wrong_operation()),
                None => self.inner.read_at(bytes, offset),
            };
            self.record(
                "read",
                action,
                action == Some(Action::ReadError),
                result.as_ref().ok().copied(),
                result.as_ref().err().map(io::Error::kind),
            );
            result
        }
        fn write_at(&mut self, bytes: &[u8], offset: u64) -> io::Result<usize> {
            let action = self.next();
            let mut applied = false;
            let result = match action {
                Some(Action::ShortWrite(n)) if n > 0 && n < bytes.len() => {
                    let result = self.inner.write_at(&bytes[..n], offset);
                    applied = matches!(result, Ok(written) if written > 0 && written <= n);
                    result
                }
                Some(Action::WriteThenError(n)) if n > 0 && n <= bytes.len() => {
                    let written = self.inner.write_at(&bytes[..n], offset);
                    applied = matches!(written, Ok(count) if count > 0 && count <= n);
                    self.record(
                        "write",
                        action,
                        applied,
                        written.as_ref().ok().copied(),
                        Some(
                            written
                                .as_ref()
                                .err()
                                .map_or(io::ErrorKind::Other, io::Error::kind),
                        ),
                    );
                    return Err(written.err().unwrap_or_else(failure));
                }
                Some(Action::ZeroWrite) if !bytes.is_empty() => {
                    applied = true;
                    Ok(0)
                }
                Some(_) => Err(wrong_operation()),
                None => self.inner.write_at(bytes, offset),
            };
            self.record(
                "write",
                action,
                applied,
                result.as_ref().ok().copied(),
                result.as_ref().err().map(io::Error::kind),
            );
            result
        }
        fn sync_all(&mut self) -> io::Result<()> {
            let action = self.next();
            let mut applied = false;
            let result = match action {
                Some(Action::SyncErrorAfterFlush) => match self.inner.sync_all() {
                    Ok(()) => {
                        applied = true;
                        Err(failure())
                    }
                    Err(error) => Err(error),
                },
                Some(_) => Err(wrong_operation()),
                None => self.inner.sync_all(),
            };
            self.record(
                "sync",
                action,
                applied,
                None,
                result.as_ref().err().map(io::Error::kind),
            );
            result
        }
        fn set_len(&mut self, len: u64) -> io::Result<()> {
            let action = self.next();
            let result = if action.is_some() {
                Err(wrong_operation())
            } else {
                self.inner.set_len(len)
            };
            self.record(
                "resize",
                action,
                false,
                None,
                result.as_ref().err().map(io::Error::kind),
            );
            result
        }
    }
}
