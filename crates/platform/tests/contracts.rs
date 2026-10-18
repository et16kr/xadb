use std::fs::{File, OpenOptions};
use xadb_foundation::{
    ErrorCode,
    codec::{decode_frame, encode_frame},
    error::Effect,
    io::{read_exact_at, resize, sync, write_all_at},
    runtime::{Cancellation, Clock, Scheduler},
};
use xadb_platform::{
    linux::{LinuxFile, sync_directory},
    runtime::{MonotonicClock, NativeScheduler},
};

fn hex(bytes: &str) -> Vec<u8> {
    bytes
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn golden_frame() -> Vec<u8> {
    // Independent Python hashlib SHA256 over the documented literal header + b"abc".
    hex(
        "41584631010003000000616263c7f1e99df28fe3fd54487543c592b6980d856f9a581e90bb7f7566d655714ce8",
    )
}

#[test]
fn frame_real_file_sync_rename_directory_sync_reopen_and_corruption() {
    let dir = tempfile::tempdir().unwrap();
    let pending = dir.path().join("pending");
    let published = dir.path().join("current");
    let mut file = LinuxFile(
        OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&pending)
            .unwrap(),
    );
    let expected = golden_frame();
    write_all_at(&mut file, &encode_frame(b"abc").unwrap(), 0).unwrap();
    sync(&mut file).unwrap();
    std::fs::rename(&pending, &published).unwrap();
    sync_directory(&File::open(dir.path()).unwrap()).unwrap();
    drop(file);
    assert_eq!(std::fs::read(&published).unwrap(), expected);
    let mut reopened = LinuxFile(
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(&published)
            .unwrap(),
    );
    let mut read = vec![0; expected.len()];
    read_exact_at(&mut reopened, &mut read, 0).unwrap();
    assert_eq!(decode_frame(&read).unwrap(), b"abc");
    write_all_at(&mut reopened, b"X", 10).unwrap();
    sync(&mut reopened).unwrap();
    read_exact_at(&mut reopened, &mut read, 0).unwrap();
    assert!(decode_frame(&read).is_err());
    resize(&mut reopened, 2).unwrap();
    assert_eq!(
        read_exact_at(&mut reopened, &mut read, 0)
            .unwrap_err()
            .io_kind,
        Some(std::io::ErrorKind::UnexpectedEof)
    );
}

#[test]
fn invalid_file_offsets_have_no_effect() {
    let mut file = LinuxFile(tempfile::tempfile().unwrap());
    assert_eq!(
        write_all_at(&mut file, b"x", i64::MAX as u64)
            .unwrap_err()
            .effect(),
        Effect::Unchanged
    );
    assert_eq!(
        resize(&mut file, u64::MAX).unwrap_err().code,
        ErrorCode::LimitExceeded
    );
    assert_eq!(file.0.metadata().unwrap().len(), 0);
    assert_eq!(
        sync_directory(&file.0).unwrap_err().code,
        ErrorCode::InvalidFormat
    );
}

#[test]
fn append_mode_must_not_silently_ignore_positional_offset() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("append");
    std::fs::write(&path, b"abc").unwrap();
    let mut file = LinuxFile(
        OpenOptions::new()
            .read(true)
            .append(true)
            .open(&path)
            .unwrap(),
    );
    let result = write_all_at(&mut file, b"X", 0);
    assert!(
        result.is_err(),
        "append handle was accepted for positional writing"
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"abc");
}

#[test]
fn cancellation_crosses_threads_and_native_clock_is_monotonic() {
    let cancellation = Cancellation::default();
    cancellation.check().unwrap();
    let other = cancellation.clone();
    std::thread::spawn(move || other.cancel()).join().unwrap();
    assert_eq!(cancellation.check().unwrap_err().code, ErrorCode::Cancelled);
    let clock = MonotonicClock::default();
    let before = clock.now_nanos();
    NativeScheduler.checkpoint(1, "yield").unwrap();
    assert!(clock.now_nanos() >= before);
}

#[cfg(feature = "test-support")]
mod faults {
    use super::*;
    use xadb_foundation::{
        error::{Certainty, RetryScope},
        io::{
            FileIo,
            fault::{Action, FaultFile},
        },
    };

    #[test]
    fn missing_or_ineffective_fault_cannot_be_qualified() {
        let file = || LinuxFile(tempfile::tempfile().unwrap());
        let mut missing = FaultFile::new(file(), 2, Action::ShortWrite(1)).unwrap();
        write_all_at(&mut missing, b"abc", 0).unwrap();
        assert!(missing.finish().is_err());
        for action in [
            Action::ShortWrite(0),
            Action::ShortWrite(3),
            Action::WriteThenError(0),
            Action::WriteThenError(4),
        ] {
            let mut invalid = FaultFile::new(file(), 1, action).unwrap();
            assert!(write_all_at(&mut invalid, b"abc", 0).is_err());
            assert_eq!(invalid.inner.0.metadata().unwrap().len(), 0);
            assert_eq!(invalid.receipts()[0].injected, None);
            assert!(invalid.finish().is_err());
        }
        let mut lost = FaultFile::new(file(), 1, Action::ReadError).unwrap();
        assert!(lost.read_at(&mut [0], 0).is_err());
        lost.finish().unwrap();
        for _ in 0..256 {
            lost.read_at(&mut [], 0).unwrap();
        }
        assert!(lost.finish().is_err());
    }

    #[test]
    fn short_write_retries_remaining_bytes_in_real_file() {
        let file = LinuxFile(tempfile::tempfile().unwrap());
        let mut fault = FaultFile::new(file, 1, Action::ShortWrite(2)).unwrap();
        write_all_at(&mut fault, b"abcdef", 3).unwrap();
        assert_eq!(fault.receipts()[0].completed_bytes, Some(2));
        assert_eq!(fault.receipts()[1].completed_bytes, Some(4));
        fault.finish().unwrap();
        let mut bytes = [0; 9];
        read_exact_at(&mut fault.inner, &mut bytes, 0).unwrap();
        assert_eq!(&bytes, b"\0\0\0abcdef");
    }

    #[test]
    fn error_after_real_partial_write_preserves_prefix_and_unknown_effect() {
        let file = LinuxFile(tempfile::tempfile().unwrap());
        let mut fault = FaultFile::new(file, 1, Action::WriteThenError(3)).unwrap();
        let error = write_all_at(&mut fault, b"abcdef", 0).unwrap_err();
        assert_eq!(error.certainty(), Certainty::Unknown);
        assert_eq!(error.retry_scope(), RetryScope::AfterRecovery);
        assert_eq!(fault.inner.0.metadata().unwrap().len(), 3);
        let mut bytes = [0; 3];
        read_exact_at(&mut fault.inner, &mut bytes, 0).unwrap();
        assert_eq!(&bytes, b"abc");
        assert_eq!(fault.receipts()[0].completed_bytes, Some(3));
        assert!(fault.receipts()[0].error.is_some());
        fault.finish().unwrap();
    }

    #[test]
    fn sync_failure_does_not_imply_that_written_data_disappeared() {
        let mut file = LinuxFile(tempfile::tempfile().unwrap());
        write_all_at(&mut file, b"durable?", 0).unwrap();
        let mut fault = FaultFile::new(file, 1, Action::SyncErrorAfterFlush).unwrap();
        assert_eq!(
            sync(&mut fault).unwrap_err().certainty(),
            Certainty::Unknown
        );
        let mut bytes = [0; 8];
        read_exact_at(&mut fault.inner, &mut bytes, 0).unwrap();
        assert_eq!(&bytes, b"durable?");
        assert_eq!(fault.receipts()[0].operation, "sync");
        fault.finish().unwrap();
    }

    #[test]
    fn zero_write_read_error_wrong_fault_and_receipt_budget_are_explicit() {
        let file = || LinuxFile(tempfile::tempfile().unwrap());
        let mut fault = FaultFile::new(file(), 1, Action::ZeroWrite).unwrap();
        assert_eq!(
            write_all_at(&mut fault, b"abc", 0).unwrap_err().io_kind,
            Some(std::io::ErrorKind::WriteZero)
        );
        let mut fault = FaultFile::new(file(), 1, Action::ReadError).unwrap();
        assert_eq!(
            read_exact_at(&mut fault, &mut [0; 1], 0).unwrap_err().code,
            ErrorCode::IoRead
        );
        let mut fault = FaultFile::new(file(), 1, Action::ReadError).unwrap();
        assert_eq!(
            write_all_at(&mut fault, b"abc", 0).unwrap_err().io_kind,
            Some(std::io::ErrorKind::InvalidInput)
        );
        for _ in 0..300 {
            let _ = fault.read_at(&mut [], 0);
        }
        assert_eq!(fault.receipts().len(), 256);
        assert_eq!(fault.dropped_receipts(), 45);
    }
}
