//! Linux file effects. Callers own paths, publication and recovery policy.
use foundation::{Error, ErrorCode, Result, io::FileIo};
use nix::fcntl::{FcntlArg, OFlag, fcntl};
use std::{fs::File, io, os::unix::fs::FileExt};

/// Owned native file adapter for explicit-offset I/O.
/// The caller owns path selection, open mode, writer exclusion, and publication.
/// Shared file status flags must not change concurrently with writes; O_APPEND is rejected.
/// Calls can block and are not cancelled by foundation tokens. Drop closes the handle
/// without syncing; use explicit sync and check its result. The real file tests in
/// `tests/contracts.rs` cover append rejection, offsets, sync, faults, and reopen.
pub struct LinuxFile(
    /// Already-open native file, whose sharing and access mode are controlled by the caller.
    pub File,
);
impl FileIo for LinuxFile {
    fn read_at(&mut self, bytes: &mut [u8], offset: u64) -> io::Result<usize> {
        self.0.read_at(bytes, offset)
    }
    fn write_at(&mut self, bytes: &[u8], offset: u64) -> io::Result<usize> {
        // Linux pwrite appends on O_APPEND handles despite its explicit offset.
        // Check every call, including flags changed through a duplicated handle.
        // The owner must not concurrently mutate shared file status flags.
        let flags = fcntl(&self.0, FcntlArg::F_GETFL)
            .map_err(|e| io::Error::from_raw_os_error(e as i32))?;
        if OFlag::from_bits_retain(flags).contains(OFlag::O_APPEND) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "O_APPEND is incompatible with positional writes",
            ));
        }
        self.0.write_at(bytes, offset)
    }
    fn sync_all(&mut self) -> io::Result<()> {
        self.0.sync_all()
    }
    fn set_len(&mut self, len: u64) -> io::Result<()> {
        self.0.set_len(len)
    }
}

/// Sync an already-owned directory after the caller's namespace changes.
/// Blocks in the OS. Caller owns create/rename ordering and must sync file contents first
/// when required. Successful sync is not power-loss qualification for an arbitrary device.
///
/// # Errors
/// Returns IoRead on metadata failure, InvalidFormat for a non-directory handle, or
/// IoWriteUnknown on sync failure. Namespace effects may already exist on failure.
pub fn sync_directory(directory: &File) -> Result<()> {
    if !directory
        .metadata()
        .map_err(|e| Error::io(ErrorCode::IoRead, "directory metadata", e))?
        .is_dir()
    {
        return Err(Error::new(
            ErrorCode::InvalidFormat,
            "directory handle required",
        ));
    }
    directory.sync_all().map_err(|e| {
        Error::io(
            ErrorCode::IoWriteUnknown,
            "directory sync outcome unknown",
            e,
        )
    })
}
