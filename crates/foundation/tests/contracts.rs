use foundation::{
    Error, ErrorCode,
    codec::{MAX_FRAME_PAYLOAD, decode_frame, encode_frame, encode_key},
    error::{Certainty, Effect, RetryScope},
    identity::{CommitSeq, Generation, ObjectHandle, ObjectId, SchemaEpoch},
    runtime::{Event, EventBuffer, EventSink, Lifecycle, Phase, Quota},
    value::{MAX_VALUE_BYTES, Value, checked_add},
};
use std::{
    cmp::Ordering,
    sync::{Arc, Barrier},
};

fn hex(bytes: &str) -> Vec<u8> {
    bytes
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn identities_reject_zero_wrap_and_reused_or_changed_objects() {
    assert_eq!(
        ObjectId::new(0).unwrap_err().code,
        ErrorCode::InvalidIdentity
    );
    assert_eq!(
        CommitSeq::new(u64::MAX).unwrap().next().unwrap_err().code,
        ErrorCode::Overflow
    );
    let handle = ObjectHandle {
        object: ObjectId::new(7).unwrap(),
        generation: Generation::new(2).unwrap(),
        schema_epoch: SchemaEpoch::new(3).unwrap(),
    };
    handle.validate_current(handle).unwrap();
    for changed in [
        ObjectHandle {
            object: ObjectId::new(8).unwrap(),
            ..handle
        },
        ObjectHandle {
            generation: handle.generation.next().unwrap(),
            ..handle
        },
        ObjectHandle {
            schema_epoch: handle.schema_epoch.next().unwrap(),
            ..handle
        },
    ] {
        assert_eq!(
            handle.validate_current(changed).unwrap_err().code,
            ErrorCode::StaleHandle
        );
    }
}

#[test]
fn independent_wide_integer_oracle_covers_signs_and_boundaries() {
    let values = [
        i64::MIN,
        i64::MIN + 1,
        -100,
        -1,
        0,
        1,
        100,
        i64::MAX - 1,
        i64::MAX,
    ];
    for left in values {
        for right in values {
            let expected = i128::from(left) + i128::from(right);
            match i64::try_from(expected) {
                Ok(value) => assert_eq!(checked_add(left, right).unwrap(), value),
                Err(_) => assert_eq!(
                    checked_add(left, right).unwrap_err().code,
                    ErrorCode::Overflow
                ),
            }
        }
    }
}

#[test]
fn null_comparison_and_no_implicit_coercion() {
    assert_eq!(Value::Null.compare(&Value::Null).unwrap(), None);
    assert_eq!(Value::Int64(5).compare(&Value::Null).unwrap(), None);
    assert_eq!(
        Value::Int64(1)
            .compare(&Value::Bool(true))
            .unwrap_err()
            .code,
        ErrorCode::TypeMismatch
    );
    assert_eq!(
        Value::Utf8("a".into())
            .compare(&Value::Utf8("b".into()))
            .unwrap(),
        Some(Ordering::Less)
    );
    // Binary UTF-8 has no collation normalization: decomposed and composed differ.
    assert_ne!(
        Value::Utf8("é".into())
            .compare(&Value::Utf8("e\u{301}".into()))
            .unwrap(),
        Some(Ordering::Equal)
    );
}

#[test]
fn key_encoding_matches_hand_calculated_vectors() {
    for (value, expected) in [
        (Value::Null, "00"),
        (Value::Bool(false), "0100"),
        (Value::Bool(true), "0101"),
        (Value::Int64(i64::MIN), "020000000000000000"),
        (Value::Int64(-1), "027fffffffffffffff"),
        (Value::Int64(0), "028000000000000000"),
        (Value::Int64(i64::MAX), "02ffffffffffffffff"),
        (Value::Utf8("".into()), "030000"),
        (Value::Utf8("a\0b".into()), "036100ff620000"),
        (Value::Bytes(vec![0, 255]), "0400ffff0000"),
    ] {
        assert_eq!(encode_key(&value).unwrap(), hex(expected));
    }
}

#[test]
fn every_key_pair_agrees_with_explicit_order_including_prefix_and_zero() {
    let ordered = [
        Value::Null,
        Value::Bool(false),
        Value::Bool(true),
        Value::Int64(i64::MIN),
        Value::Int64(-1),
        Value::Int64(0),
        Value::Int64(1),
        Value::Int64(i64::MAX),
        Value::Utf8("".into()),
        Value::Utf8("\0".into()),
        Value::Utf8("a".into()),
        Value::Utf8("a\0".into()),
        Value::Utf8("aa".into()),
        Value::Utf8("é".into()),
        Value::Bytes(vec![]),
        Value::Bytes(vec![0]),
        Value::Bytes(vec![0, 0]),
        Value::Bytes(vec![0, 255]),
        Value::Bytes(vec![1]),
        Value::Bytes(vec![255]),
    ];
    for (i, left) in ordered.iter().enumerate() {
        for (j, right) in ordered.iter().enumerate() {
            assert_eq!(
                encode_key(left).unwrap().cmp(&encode_key(right).unwrap()),
                i.cmp(&j)
            );
            assert_eq!(left.cmp(right), i.cmp(&j));
        }
    }
}

#[test]
fn value_budget_checks_bytes_not_characters() {
    assert!(encode_key(&Value::Bytes(vec![0; MAX_VALUE_BYTES])).is_ok());
    assert_eq!(
        encode_key(&Value::Bytes(vec![0; MAX_VALUE_BYTES + 1]))
            .unwrap_err()
            .code,
        ErrorCode::LimitExceeded
    );
    assert!(
        Value::Utf8("é".repeat(MAX_VALUE_BYTES / 2))
            .validate()
            .is_ok()
    );
    assert_eq!(
        Value::Utf8("é".repeat(MAX_VALUE_BYTES / 2 + 1))
            .validate()
            .unwrap_err()
            .code,
        ErrorCode::LimitExceeded
    );
}

fn golden_frame() -> Vec<u8> {
    // Independent Python hashlib SHA256 over the documented literal header + b"abc".
    hex(
        "41584631010003000000616263c7f1e99df28fe3fd54487543c592b6980d856f9a581e90bb7f7566d655714ce8",
    )
}

#[test]
fn frame_matches_independent_golden_and_rejects_every_truncation_and_bit_flip() {
    let expected = golden_frame();
    assert_eq!(encode_frame(b"abc").unwrap(), expected);
    assert_eq!(decode_frame(&expected).unwrap(), b"abc");
    for cut in 0..expected.len() {
        assert!(decode_frame(&expected[..cut]).is_err());
    }
    for index in 0..expected.len() {
        for bit in 0..8 {
            let mut corrupt = expected.clone();
            corrupt[index] ^= 1 << bit;
            assert!(
                decode_frame(&corrupt).is_err(),
                "accepted corruption {index}/{bit}"
            );
        }
    }
    let mut trailing = expected.clone();
    trailing.push(0);
    assert!(decode_frame(&trailing).is_err());
}

#[test]
fn frame_limits_and_unsupported_format_are_explicit() {
    let max = vec![0x5a; MAX_FRAME_PAYLOAD];
    assert_eq!(decode_frame(&encode_frame(&max).unwrap()).unwrap(), max);
    assert_eq!(
        encode_frame(&vec![0; MAX_FRAME_PAYLOAD + 1])
            .unwrap_err()
            .code,
        ErrorCode::LimitExceeded
    );
    let mut frame = golden_frame();
    frame[4] = 2;
    assert_eq!(
        decode_frame(&frame).unwrap_err().code,
        ErrorCode::UnsupportedFormat
    );
    frame[4] = 1;
    frame[6..10].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        decode_frame(&frame).unwrap_err().code,
        ErrorCode::LimitExceeded
    );
    assert_eq!(decode_frame(&encode_frame(&[]).unwrap()).unwrap(), b"");
}

#[test]
fn uncertain_io_is_never_an_automatic_retry_or_rollback() {
    let unknown = Error::new(ErrorCode::IoWriteUnknown, "write");
    assert_eq!(unknown.effect(), Effect::CallerMustResolve);
    assert_eq!(unknown.certainty(), Certainty::Unknown);
    assert_eq!(unknown.retry_scope(), RetryScope::AfterRecovery);
    let validation = Error::new(ErrorCode::Overflow, "add");
    assert_eq!(validation.effect(), Effect::Unchanged);
    assert_eq!(validation.certainty(), Certainty::Known);
    assert_eq!(validation.retry_scope(), RetryScope::Never);
    assert_eq!(validation.code.id(), "XADB-FOUND-0003");
}

#[test]
fn reservations_release_on_error_and_cannot_overflow() {
    let quota = Quota::new(u64::MAX);
    let all = quota.reserve(u64::MAX).unwrap();
    assert!(quota.reserve(1).is_err());
    assert_eq!(quota.used(), u64::MAX);
    drop(all);
    let operation = || -> foundation::Result<()> {
        let _reservation = quota.reserve(10)?;
        Err(Error::new(ErrorCode::Cancelled, "test"))
    };
    assert!(operation().is_err());
    assert_eq!(quota.used(), 0);
}

#[test]
fn real_threads_cannot_overbook_quota() {
    let quota = Quota::new(100);
    let start = Arc::new(Barrier::new(33));
    let reserved = Arc::new(Barrier::new(33));
    let release = Arc::new(Barrier::new(33));
    let mut handles = vec![];
    for _ in 0..32 {
        let (quota, start, reserved, release) = (
            quota.clone(),
            start.clone(),
            reserved.clone(),
            release.clone(),
        );
        handles.push(std::thread::spawn(move || {
            start.wait();
            let reservation = quota.reserve(10);
            reserved.wait();
            release.wait();
            reservation.is_ok()
        }));
    }
    start.wait();
    reserved.wait();
    assert_eq!(quota.used(), 100);
    release.wait();
    assert_eq!(
        handles
            .into_iter()
            .map(|h| usize::from(h.join().unwrap()))
            .sum::<usize>(),
        10
    );
    assert_eq!(quota.used(), 0);
}

#[test]
fn stop_closes_admission_then_drains_existing_work() {
    let runtime = Lifecycle::default();
    let guard = runtime.admit().unwrap();
    assert_eq!(runtime.stop().unwrap(), Phase::Draining);
    assert!(matches!(
        runtime.admit(),
        Err(Error {
            code: ErrorCode::Closed,
            ..
        })
    ));
    drop(guard);
    assert_eq!(runtime.phase().unwrap(), Phase::Stopped);
    assert_eq!(runtime.stop().unwrap(), Phase::Stopped);
    assert!(runtime.admit().is_err());
}

#[test]
fn diagnostics_are_bounded_and_report_loss() {
    let mut events = EventBuffer::new(2).unwrap();
    for id in 1..=3 {
        events.emit(Event {
            operation_id: id,
            object: None,
            at_nanos: 4,
            event_id: "XADB-FOUND-IO",
            error: Some(ErrorCode::IoWriteUnknown),
        });
    }
    assert_eq!(events.dropped(), 1);
    assert_eq!(events.pop().unwrap().operation_id, 1);
    assert_eq!(events.pop().unwrap().operation_id, 2);
    assert_eq!(events.pop(), None);
    assert!(EventBuffer::new(0).is_err());
    assert!(EventBuffer::new(4097).is_err());
}

#[cfg(feature = "test-support")]
mod faults {
    use super::*;
    use foundation::{
        io::{
            FileIo,
            fault::{Action, FaultFile},
            sync,
        },
        runtime::{
            Clock, Scheduler,
            controlled::{ManualClock, ScriptedScheduler},
        },
    };

    #[test]
    fn rejected_schedule_cannot_later_finish_successfully() {
        let schedule = ScriptedScheduler::new(vec![(1, "write")]).unwrap();
        assert!(schedule.checkpoint(2, "wrong").is_err());
        let _ = schedule.checkpoint(1, "write");
        assert!(
            schedule.finish().is_err(),
            "earlier schedule violation was forgotten"
        );
    }

    #[test]
    fn failed_real_sync_is_not_reported_as_an_injected_post_sync_failure() {
        struct FailedSync;
        impl FileIo for FailedSync {
            fn read_at(&mut self, _: &mut [u8], _: u64) -> std::io::Result<usize> {
                unreachable!()
            }
            fn write_at(&mut self, _: &[u8], _: u64) -> std::io::Result<usize> {
                unreachable!()
            }
            fn sync_all(&mut self) -> std::io::Result<()> {
                Err(std::io::Error::other("underlying sync failed"))
            }
            fn set_len(&mut self, _: u64) -> std::io::Result<()> {
                unreachable!()
            }
        }
        let mut fault = FaultFile::new(FailedSync, 1, Action::SyncErrorAfterFlush).unwrap();
        assert!(sync(&mut fault).is_err());
        assert_eq!(
            fault.receipts()[0].injected,
            None,
            "failed sync was incorrectly claimed as a successful flush followed by injection"
        );
        assert!(fault.finish().is_err());
    }

    #[test]
    fn manual_clock_and_schedule_reject_overflow_missing_or_wrong_steps() {
        let clock = ManualClock::default();
        clock.advance(u64::MAX).unwrap();
        assert_eq!(clock.advance(1).unwrap_err().code, ErrorCode::Overflow);
        assert_eq!(clock.now_nanos(), u64::MAX);
        let schedule = ScriptedScheduler::new(vec![(1, "prepare"), (2, "sync")]).unwrap();
        assert!(schedule.finish().is_err());
        assert!(schedule.checkpoint(2, "sync").is_err());
        assert!(schedule.finish().is_err());
        let schedule = ScriptedScheduler::new(vec![(1, "prepare"), (2, "sync")]).unwrap();
        schedule.checkpoint(1, "prepare").unwrap();
        schedule.checkpoint(2, "sync").unwrap();
        schedule.finish().unwrap();
        assert!(schedule.checkpoint(2, "sync").is_err());
        assert!(schedule.finish().is_err());
    }
}
