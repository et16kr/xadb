//! Actual owner histories compared with independent literal allocator/WAL bytes.
use super::super::super::{InitAttemptError, InitializedStatusObservation, OPERATION_BYTES};
use super::*;
use crate::storage::{
    dirty_frame::{self, DirtyFrame, init_test_support as frame_init},
    page::{PAGE_BYTES, PageLsn},
    wal_io::{self as wal, FreshStorageSession, bootstrap::test_support as port},
};
use crate::transaction::concurrency::{OwnerRole, PageConcurrency};

const INPUT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-operational-status"
);
pub(super) const HOME: &str = "d3-data-0000000100000001";

pub(super) fn boxed<const N: usize>(byte: u8) -> Box<[u8; N]> {
    vec![byte; N].into_boxed_slice().try_into().unwrap()
}
pub(super) fn literal(name: &str) -> Vec<u8> {
    std::fs::read(format!("{INPUT}/p1/{name}")).unwrap()
}
pub(super) fn converted<B, E>(result: std::result::Result<B, IntoOperationalStatusError<E>>) -> B {
    match result {
        Ok(bound) => bound,
        Err(IntoOperationalStatusError::Before { error, .. })
        | Err(IntoOperationalStatusError::Fatal(error)) => panic!("conversion {error:?}"),
    }
}
pub(super) fn initialized<W: WriterNamespace>(
    reserved: ReservedStatus<'_, '_, '_, '_, '_, '_, '_, '_, '_, W>,
) -> InitializedStatusObservation {
    match reserved.initialize() {
        Ok(result) => result,
        Err(InitAttemptError::Before(error)) => panic!("INIT before {:?}", error.error),
        Err(InitAttemptError::Fatal(error)) => panic!("INIT fatal {error:?}"),
    }
}
macro_rules! fixture {
    (|$original:ident, $core:ident, $state:ident, $storage:ident| $body:block) => {
        fixture!(1, |$original, $core, $state, $storage| $body);
    };
    ($profile:expr, |$original:ident, $core:ident, $state:ident, $storage:ident| $body:block) => {{
        let mut slot = port::slot();
        let mut flush_workspace = boxed::<{ 4 << 20 }>(0);
        let (namespace, $state) = port::fresh();
        let fresh = FreshStorageSession::create_status(namespace, $profile, &mut slot, &mut [[0; PAGE_BYTES]; 3]).unwrap();
        let installed = fresh.install_doublewrite(&mut flush_workspace).unwrap();
        let $storage = installed.into_mutable_storage();
        let $core = PageConcurrency::new($storage.session_fence()).unwrap();
        let mut allocator = boxed::<PAGE_BYTES>(0x2d);
        let mut payload = boxed::<PAGE_BYTES>(0xa5);
        let mut operation = boxed::<OPERATION_BYTES>(0x73);
        let mut frames = [DirtyFrame::new(&mut allocator), DirtyFrame::new(&mut payload)];
        let bound = $storage.bind_status_bootstrap(&$core, &mut frames, &mut operation).unwrap();
        let $original = match bound.into_with_flush() {
            Ok(bound) => bound,
            Err(crate::storage::wal_io::IntoFlushError::Before { error, .. }) | Err(crate::storage::wal_io::IntoFlushError::Fatal(error)) => panic!("flush conversion {error:?}"),
        };
        $body
        assert!($storage.session_fence().is_fenced());
        drop($storage);
        let state = $state.lock().unwrap();
        assert_eq!((state.handles, state.writers), (0, 0));
    }};
}

pub(super) use fixture;

#[test]
fn operational_status_resource_gate() {
    type W = port::Writer;
    println!("conversion {} <= 65536", conversion_bytes::<W>());
    println!(
        "original_bound {}",
        size_of::<BoundStatusWithFlush<'_, '_, '_, '_, '_, '_, '_, W>>()
    );
    println!(
        "operational_bound {}",
        size_of::<OperationalStatus<'_, '_, '_, '_, '_, '_, '_, W>>()
    );
    for (name, value) in frame_init::init_layout() {
        println!("{name} {value}");
    }
    assert!(fits::<W>());
    assert!(dirty_frame::fits());
    assert!(dirty_frame::init_fits());
    assert_eq!(
        size_of::<OperationalStatus<'_, '_, '_, '_, '_, '_, '_, W>>(),
        size_of::<BoundStatusWithFlush<'_, '_, '_, '_, '_, '_, '_, W>>()
    );
}

#[test]
fn operational_status_repeated_init_preserves_literal_history() {
    fixture!(|original, core, state, storage| {
        let mut bound = converted(original.into_operational());
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let mut flush_owner = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let first = {
            let (mut writer, reader, mut flush) = bound.split_with_flush();
            let first = initialized(writer.reserve_one(&mut owner).unwrap());
            assert_eq!(
                (
                    first.page.page_no,
                    first.page.incarnation.get(),
                    first.lsn.0,
                    first.status.generation
                ),
                (1, 2, 2, 1)
            );
            let read = reader.read(&mut owner).unwrap();
            assert_eq!(
                read.bytes().unwrap().as_slice(),
                literal("work1-status.after")
            );
            read.release_checked().unwrap();
            assert!(matches!(
                flush.try_flush_status(&mut flush_owner).unwrap(),
                super::super::FlushProgress::Complete(_)
            ));
            assert!(matches!(
                flush.try_flush_allocator(&mut flush_owner).unwrap(),
                super::super::FlushProgress::Complete(_)
            ));
            first
        };
        let old_request = bound
            .inner
            .inner
            .pool
            .initialized_request()
            .unwrap()
            .unwrap();
        let events = state.lock().unwrap().events.len();
        let evicted = bound.prepare_next_status().unwrap();
        assert_eq!(
            (evicted.identity, evicted.generation),
            (first.status.identity, 1)
        );
        assert_eq!(state.lock().unwrap().events.len(), events);
        assert_eq!(bound.inner.inner.pool.init_generations().unwrap(), [1, 2]);
        assert!(
            bound
                .inner
                .inner
                .pool
                .initialized_request()
                .unwrap()
                .is_none()
        );
        let stale = bound
            .inner
            .inner
            .pool
            .reserve_targets(&mut owner, &[old_request]);
        assert!(matches!(
            stale,
            Err(dirty_frame::DirtyError {
                cause: dirty_frame::Cause::MissingTarget,
                effect: Effect::Unchanged
            })
        ));
        drop(stale);
        assert!(bound.prepare_next_status().is_err());
        assert_eq!(bound.inner.inner.pool.init_generations().unwrap(), [1, 2]);
        {
            let (mut writer, _, _) = bound.split_with_flush();
            let reservation = writer.reserve_one(&mut owner).unwrap();
            assert_eq!(
                (
                    reservation.page().page_no,
                    reservation.page().incarnation.get()
                ),
                (2, 2)
            );
            match reservation.cancel() {
                Ok(observation) => assert_eq!(
                    (observation.work_high_water, observation.page_lsn.0),
                    (2, 4)
                ),
                Err(InitAttemptError::Before(error)) => panic!("cancel {:?}", error.error),
                Err(InitAttemptError::Fatal(error)) => panic!("cancel {error:?}"),
            }
        }
        assert_eq!(
            bound.inner.inner.control.base.descriptor.as_slice(),
            literal("work2-terminal.descriptor")
        );
        {
            let (mut writer, reader, mut flush) = bound.split_with_flush();
            let second = initialized(writer.reserve_one(&mut owner).unwrap());
            assert_eq!(
                (
                    second.page.page_no,
                    second.page.incarnation.get(),
                    second.lsn.0,
                    second.status.generation
                ),
                (2, 3, 6, 2)
            );
            assert_eq!(second.allocator.modification_counter, 6);
            assert_eq!(second.status.modification_counter, 1);
            let read = reader.read(&mut owner).unwrap();
            assert_eq!(
                read.bytes().unwrap().as_slice(),
                literal("work3-status.after")
            );
            read.release_checked().unwrap();
            flush.try_flush_allocator(&mut flush_owner).unwrap();
            flush.try_flush_status(&mut flush_owner).unwrap();
        }
        let mut allocator = boxed::<PAGE_BYTES>(0);
        frame_init::snapshot(&bound.inner.inner.pool, 0, &mut allocator);
        assert_eq!(allocator.as_slice(), literal("work3-allocator.after"));
        assert_eq!(
            bound.inner.inner.control.base.descriptor.as_slice(),
            literal("work3-terminal.descriptor")
        );
        assert_eq!(bound.inner.inner.control.base.work_high_water, 3);
        {
            let state = state.lock().unwrap();
            assert_eq!(
                &state.files[wal::WAL_NAME].bytes[4096..],
                literal("wal.frames-only")
            );
            let home = &state.files[HOME].bytes;
            assert_eq!(&home[..PAGE_BYTES], literal("work3-allocator.after"));
            assert_eq!(
                &home[PAGE_BYTES..2 * PAGE_BYTES],
                literal("work1-status.after")
            );
            assert_eq!(
                &home[2 * PAGE_BYTES..3 * PAGE_BYTES],
                literal("work3-status.after")
            );
        }
        assert!(!storage.session_fence().is_fenced());
        drop(bound);
    });
}

#[test]
fn operational_status_dirty_conversion_returns_exact_owner() {
    fixture!(|original, core, state, storage| {
        let mut original = original;
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let (mut writer, _, _) = original.split_with_flush();
        initialized(writer.reserve_one(&mut owner).unwrap());
        let descriptor = original.inner.control.base.descriptor;
        let events = state.lock().unwrap().events.len();
        let mut original = match original.into_operational() {
            Err(IntoOperationalStatusError::Before { error, bound }) => {
                assert_eq!(error.effect, Effect::Unchanged);
                bound
            }
            _ => panic!("dirty conversion must preserve owner"),
        };
        assert_eq!(original.inner.control.base.descriptor, descriptor);
        assert_eq!(state.lock().unwrap().events.len(), events);
        let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let (_, _, mut flush) = original.split_with_flush();
        flush.try_flush_status(&mut flusher).unwrap();
        flush.try_flush_allocator(&mut flusher).unwrap();
        let mut bound = converted(original.into_operational());
        bound.prepare_next_status().unwrap();
        let (mut writer, _, _) = bound.split_with_flush();
        let next = initialized(writer.reserve_one(&mut owner).unwrap());
        assert_eq!(
            (next.page.page_no, next.lsn, next.status.generation),
            (2, PageLsn(4), 2)
        );
        assert!(!storage.session_fence().is_fenced());
        drop(bound);
    });
}

fn snapshot(
    pool: &dirty_frame::SharedPool<'_, '_, '_, '_>,
) -> Vec<(frame_init::RawSnapshot, Vec<u8>)> {
    (0..2)
        .map(|index| {
            let mut bytes = boxed::<PAGE_BYTES>(0);
            let metadata = frame_init::snapshot(pool, index, &mut bytes);
            (metadata, bytes.to_vec())
        })
        .collect()
}
fn frame_refusal(error: StatusFlushError, expected: dirty_frame::Cause) {
    assert_eq!(error.effect, Effect::Unchanged);
    match error.cause {
        Cause::Frame(source) => {
            assert_eq!(source.cause, expected);
            assert_eq!(source.effect, Effect::Unchanged);
        }
        other => panic!("expected frame refusal, got {other:?}"),
    }
}

#[test]
fn operational_status_retirement_dirty_empty_and_exhausted_are_unchanged() {
    fixture!(|original, core, state, storage| {
        let mut bound = converted(original.into_operational());
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        frame_refusal(
            bound.prepare_next_status().unwrap_err(),
            dirty_frame::Cause::MissingTarget,
        );
        {
            let (mut writer, _, _) = bound.split_with_flush();
            initialized(writer.reserve_one(&mut owner).unwrap());
        }
        let before = snapshot(&bound.inner.inner.pool);
        let descriptor = bound.inner.inner.control.base.descriptor;
        let events = state.lock().unwrap().events.len();
        frame_refusal(
            bound.prepare_next_status().unwrap_err(),
            dirty_frame::Cause::Busy,
        );
        assert_eq!(snapshot(&bound.inner.inner.pool), before);
        assert_eq!(bound.inner.inner.control.base.descriptor, descriptor);
        assert_eq!(bound.inner.inner.pool.init_generations().unwrap(), [1, 1]);
        assert_eq!(state.lock().unwrap().events.len(), events);
        {
            let (_, _, mut flush) = bound.split_with_flush();
            flush.try_flush_status(&mut dedicated).unwrap();
            flush.try_flush_allocator(&mut dedicated).unwrap();
        }
        // Test-only counter exhaustion injection; all live bytes remain real INIT output.
        frame_init::seed_status_generation(&mut bound.inner.inner.pool, u64::MAX);
        let before = snapshot(&bound.inner.inner.pool);
        let events = state.lock().unwrap().events.len();
        frame_refusal(
            bound.prepare_next_status().unwrap_err(),
            dirty_frame::Cause::CounterExhausted,
        );
        assert_eq!(snapshot(&bound.inner.inner.pool), before);
        assert_eq!(
            bound.inner.inner.pool.init_generations().unwrap(),
            [1, u64::MAX]
        );
        assert_eq!(state.lock().unwrap().events.len(), events);
        assert!(!storage.session_fence().is_fenced());
        drop(bound);
    });
}

#[test]
fn operational_status_forgotten_pin_prevents_retirement() {
    fixture!(|original, core, state, storage| {
        let mut bound = converted(original.into_operational());
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        {
            let (mut writer, _, mut flush) = bound.split_with_flush();
            initialized(writer.reserve_one(&mut owner).unwrap());
            flush.try_flush_status(&mut dedicated).unwrap();
            flush.try_flush_allocator(&mut dedicated).unwrap();
        }
        let request = bound
            .inner
            .inner
            .pool
            .initialized_request()
            .unwrap()
            .unwrap();
        let pins = bound
            .inner
            .inner
            .pool
            .reserve_targets(&mut owner, &[request])
            .unwrap();
        std::mem::forget(pins);
        let before = snapshot(&bound.inner.inner.pool);
        assert_eq!(before[1].0.total, 1);
        let events = state.lock().unwrap().events.len();
        frame_refusal(
            bound.prepare_next_status().unwrap_err(),
            dirty_frame::Cause::Busy,
        );
        assert_eq!(snapshot(&bound.inner.inner.pool), before);
        assert_eq!(bound.inner.inner.pool.init_generations().unwrap(), [1, 1]);
        assert_eq!(state.lock().unwrap().events.len(), events);
        assert!(!storage.session_fence().is_fenced());
        drop(bound);
    });
}

#[test]
fn operational_status_forgotten_init_claim_prevents_retirement() {
    fixture!(|original, core, state, storage| {
        let mut bound = converted(original.into_operational());
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let (mut writer, _, _) = bound.split_with_flush();
        let reservation = writer.reserve_one(&mut owner).unwrap();
        let authority = reservation
            .writer
            .authority(
                reservation.batch,
                reservation.page,
                reservation.ordinal,
                reservation.kind,
            )
            .unwrap();
        let pins = reservation
            .writer
            .pool
            .claim_init_targets(reservation.owner, &authority)
            .unwrap();
        std::mem::forget(pins);
        std::mem::forget(reservation);
        let before = snapshot(&bound.inner.inner.pool);
        assert!(before[0].0.claim_present && before[0].0.initializer_pins_held);
        assert_eq!(before[0].0.total, 2);
        let events = state.lock().unwrap().events.len();
        let error = bound.prepare_next_status().unwrap_err();
        assert!(matches!(error.cause, Cause::Busy));
        assert_eq!(error.effect, Effect::Unchanged);
        // Independently reach the leaf guard below the owner's pending-batch guard.
        let error = bound.inner.inner.pool.prepare_next_status().unwrap_err();
        assert_eq!(
            (error.cause, error.effect),
            (dirty_frame::Cause::Busy, Effect::Unchanged)
        );
        assert_eq!(snapshot(&bound.inner.inner.pool), before);
        assert_eq!(bound.inner.inner.pool.init_generations().unwrap(), [1, 1]);
        assert_eq!(state.lock().unwrap().events.len(), events);
        assert!(!storage.session_fence().is_fenced());
        drop(bound);
    });
}

#[test]
fn operational_status_retired_flush_must_finalize_before_eviction() {
    fixture!(|original, core, state, storage| {
        let mut bound = converted(original.into_operational());
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        {
            let (mut writer, reader, mut flush) = bound.split_with_flush();
            initialized(writer.reserve_one(&mut owner).unwrap());
            let read = reader.read(&mut owner).unwrap();
            assert!(matches!(
                flush.try_flush_status(&mut dedicated).unwrap(),
                super::super::FlushProgress::RetiredPendingFinalize
            ));
            read.release_checked().unwrap();
        }
        let before = snapshot(&bound.inner.inner.pool);
        let events = state.lock().unwrap().events.len();
        let error = bound.prepare_next_status().unwrap_err();
        assert!(matches!(error.cause, Cause::Busy));
        assert_eq!(error.effect, Effect::Unchanged);
        assert_eq!(snapshot(&bound.inner.inner.pool), before);
        assert_eq!(state.lock().unwrap().events.len(), events);
        {
            let (_, _, mut flush) = bound.split_with_flush();
            assert!(matches!(
                flush.continue_flush(&mut dedicated).unwrap(),
                super::super::FlushProgress::Complete(_)
            ));
            flush.try_flush_allocator(&mut dedicated).unwrap();
        }
        bound.prepare_next_status().unwrap();
        let before = snapshot(&bound.inner.inner.pool);
        let events = state.lock().unwrap().events.len();
        frame_refusal(
            bound.prepare_next_status().unwrap_err(),
            dirty_frame::Cause::MissingTarget,
        );
        assert_eq!(bound.inner.inner.pool.init_generations().unwrap(), [1, 2]);
        assert_eq!(snapshot(&bound.inner.inner.pool), before);
        assert_eq!(state.lock().unwrap().events.len(), events);
        assert!(!storage.session_fence().is_fenced());
        drop(bound);
    });
}

#[test]
fn operational_status_stale_init_admission_is_unchanged() {
    fixture!(|original, core, state, storage| {
        let mut bound = converted(original.into_operational());
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        {
            let (mut writer, _, mut flush) = bound.split_with_flush();
            initialized(writer.reserve_one(&mut owner).unwrap());
            flush.try_flush_status(&mut dedicated).unwrap();
            flush.try_flush_allocator(&mut dedicated).unwrap();
        }
        bound.prepare_next_status().unwrap();
        let (mut writer, _, _) = bound.split_with_flush();
        let reservation = writer.reserve_one(&mut owner).unwrap();
        let mut authority = reservation
            .writer
            .authority(
                reservation.batch,
                reservation.page,
                reservation.ordinal,
                reservation.kind,
            )
            .unwrap();
        assert_eq!(authority.generations, [1, 2]);
        authority.generations[1] = 1;
        let before = snapshot(reservation.writer.pool);
        let events = state.lock().unwrap().events.len();
        assert!(matches!(
            reservation
                .writer
                .pool
                .claim_init_targets(reservation.owner, &authority),
            Err(dirty_frame::DirtyError {
                cause: dirty_frame::Cause::Stale,
                effect: Effect::Unchanged
            })
        ));
        assert_eq!(snapshot(reservation.writer.pool), before);
        assert_eq!(state.lock().unwrap().events.len(), events);
        // Rejection must leave the genuine B reservation usable.
        assert_eq!(initialized(reservation).status.generation, 2);
        assert!(!storage.session_fence().is_fenced());
        drop(bound);
    });
}

#[test]
fn operational_status_stale_claim_release_preserves_pin_counts() {
    fixture!(|original, core, state, storage| {
        let mut bound = converted(original.into_operational());
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let (mut writer, _, _) = bound.split_with_flush();
        let reservation = writer.reserve_one(&mut owner).unwrap();
        let authority = reservation
            .writer
            .authority(
                reservation.batch,
                reservation.page,
                reservation.ordinal,
                reservation.kind,
            )
            .unwrap();
        let targets = reservation
            .writer
            .pool
            .claim_init_targets(reservation.owner, &authority)
            .unwrap();
        frame_init::corrupt_init_claim(reservation.writer.pool, 4);
        let before = snapshot(reservation.writer.pool);
        let events = state.lock().unwrap().events.len();
        let error = targets.release_checked().unwrap_err();
        assert_eq!(
            (error.cause, error.effect),
            (dirty_frame::Cause::Invariant, Effect::CallerMustResolve)
        );
        assert_eq!(snapshot(reservation.writer.pool), before);
        assert_eq!(state.lock().unwrap().events.len(), events);
        assert!(storage.session_fence().is_fenced());
        drop(reservation);
        drop(bound);
    });
}

#[test]
fn operational_status_preflight_rejects_stale_local_generation_before_stage() {
    fixture!(|original, core, state, storage| {
        let mut bound = converted(original.into_operational());
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let (mut writer, _, _) = bound.split_with_flush();
        let reservation = writer.reserve_one(&mut owner).unwrap();
        let authority = reservation
            .writer
            .authority(
                reservation.batch,
                reservation.page,
                reservation.ordinal,
                reservation.kind,
            )
            .unwrap();
        let mut write = reservation
            .writer
            .pool
            .claim_init_targets(reservation.owner, &authority)
            .unwrap()
            .try_write()
            .unwrap();
        let mut allocator = boxed::<PAGE_BYTES>(0);
        let before = write.copy_allocator_snapshot(&mut allocator).unwrap();
        frame_init::corrupt_local_init_generation(&mut write);
        let events = state.lock().unwrap().events.len();
        let error = write.preflight(&before).unwrap_err();
        assert_eq!(
            (error.cause, error.effect),
            (dirty_frame::Cause::Invariant, Effect::CallerMustResolve)
        );
        assert_eq!(state.lock().unwrap().events.len(), events);
        assert!(storage.session_fence().is_fenced());
        drop(write);
        assert_eq!(snapshot(reservation.writer.pool)[0].1, allocator.as_slice());
        drop(reservation);
        drop(bound);
    });
}

#[test]
fn operational_status_population_and_terminal_are_trusted_inputs() {
    use super::super::super::super::super::allocation_reservation::operation::{
        self, AllocationPopulation as P, OperationBudget as B, TerminalOutcome as T,
    };
    use crate::storage::allocation_page;
    fixture!(|original, core, state, storage| {
        let bound = converted(original.into_operational());
        let base = &bound.inner.inner.control.base;
        let profile = storage.profile();
        let genesis = literal("work1-allocator.before");
        let genesis = allocation_page::decode_allocation_page(
            &genesis,
            base.identity,
            profile.spec().namespace,
        )
        .unwrap();
        for (budget, terminal) in [
            (B::Exclusive81920, T::Cancelled),
            (B::Dirty98304, T::Cancelled),
            (B::StatusInit114688, T::InitializedStatus),
        ] {
            assert_eq!(
                operation::validate_live_in_population(
                    &genesis,
                    profile,
                    None,
                    &[0; 1536],
                    0,
                    budget,
                    P::Status,
                    terminal
                ),
                Err(operation::SemanticError::StateMismatch)
            );
        }
        for (work, correct, wrong) in [
            (2, T::Cancelled, T::InitializedStatus),
            (3, T::InitializedStatus, T::Cancelled),
        ] {
            let bytes = literal(&format!("work{work}-allocator.after"));
            let page = allocation_page::decode_allocation_page(
                &bytes,
                base.identity,
                profile.spec().namespace,
            )
            .unwrap();
            let descriptor = literal(&format!("work{work}-terminal.descriptor"));
            operation::validate_live_in_population(
                &page,
                profile,
                None,
                &descriptor,
                work,
                B::StatusInit114688,
                P::Status,
                correct,
            )
            .unwrap();
            assert!(
                operation::validate_live_in_population(
                    &page,
                    profile,
                    None,
                    &descriptor,
                    work,
                    B::StatusInit114688,
                    P::Status,
                    wrong
                )
                .is_err()
            );
            assert!(
                operation::validate_live_in_population(
                    &page,
                    profile,
                    None,
                    &descriptor,
                    work,
                    B::StatusInit114688,
                    P::Empty,
                    correct
                )
                .is_err()
            );
        }
        // Each mutation preserves the checksum and codec-valid cell encoding.
        for (offset, value) in [
            (10144 + 64 + 96 + 16, 3),
            (10144 + 64 + 96 + 24, 2),
            (10144 + 64 + 96 + 8, 1),
        ] {
            let mut bytes = literal("work3-allocator.after");
            bytes[offset..offset + 8].copy_from_slice(&u64::to_le_bytes(value));
            let checksum = foundation::codec::content_digest(&bytes[..PAGE_BYTES - 32]);
            bytes[PAGE_BYTES - 32..].copy_from_slice(&checksum);
            let page = allocation_page::decode_allocation_page(
                &bytes,
                base.identity,
                profile.spec().namespace,
            )
            .unwrap();
            assert_eq!(
                operation::validate_population(&page, None, P::Status),
                Err(operation::SemanticError::PageMismatch)
            );
        }
        assert!(!storage.session_fence().is_fenced());
        drop(bound);
    });
}

#[test]
fn operational_status_reader_accepts_independent_page_lsns() {
    use super::super::super::operation;
    fixture!(|original, core, state, storage| {
        let mut bound = converted(original.into_operational());
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let first = {
            let (mut writer, _, _) = bound.split_with_flush();
            initialized(writer.reserve_one(&mut owner).unwrap())
        };
        let allocator: [u8; PAGE_BYTES] = literal("work3-allocator.after").try_into().unwrap();
        let status: [u8; PAGE_BYTES] = literal("work1-status.after").try_into().unwrap();
        assert_eq!(
            operation::observe_operational_initialized(
                storage.profile(),
                bound.inner.inner.control.base.identity,
                first.status.identity,
                &storage.status_role().unwrap(),
                &allocator,
                &status
            )
            .unwrap(),
            (PageLsn(6), PageLsn(2))
        );
        // Closed observation must keep its original one-member/equal-LSN contract.
        assert!(
            operation::observe_initialized(
                storage.profile(),
                bound.inner.inner.control.base.identity,
                first.status.identity,
                &storage.status_role().unwrap(),
                &allocator,
                &status
            )
            .is_err()
        );
        assert!(!storage.session_fence().is_fenced());
        drop(bound);
    });
}

// Drive the existing sealed producer to a real stage, then corrupt only the
// tested capability. This is not a synthetic successful storage authority.
fn stale_staged_generation(completion: bool) {
    use super::super::super::{
        CompletedInitAuthority, InitControlCommit, InitInstallPermit, InitInstallPhase, operation,
    };
    fixture!(|original, core, state, storage| {
        let mut bound = converted(original.into_operational());
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        {
            let (mut writer, _, mut flush) = bound.split_with_flush();
            initialized(writer.reserve_one(&mut owner).unwrap());
            flush.try_flush_status(&mut dedicated).unwrap();
            flush.try_flush_allocator(&mut dedicated).unwrap();
        }
        bound.prepare_next_status().unwrap();
        let (mut writer, _, _) = bound.split_with_flush();
        let reservation = writer.reserve_one(&mut owner).unwrap();
        let mut authority = reservation
            .writer
            .authority(
                reservation.batch,
                reservation.page,
                reservation.ordinal,
                reservation.kind,
            )
            .unwrap();
        assert_eq!(authority.generations, [1, 2]);
        let before = snapshot(reservation.writer.pool);
        let descriptor_before = reservation.writer.base.descriptor;
        let mut write = reservation
            .writer
            .pool
            .claim_init_targets(reservation.owner, &authority)
            .unwrap()
            .try_write()
            .unwrap();
        let mut old = boxed::<PAGE_BYTES>(0);
        let mut frame = boxed::<65536>(0);
        let mut allocator_after = boxed::<PAGE_BYTES>(0);
        let mut status_after = boxed::<PAGE_BYTES>(0);
        let stamp = write.copy_allocator_snapshot(&mut old).unwrap();
        let plan = operation::InitPlan {
            profile: storage.profile(),
            allocator: reservation.writer.base.identity,
            payload: authority.payload.identity,
            prefix: storage.wal().prefix().unwrap(),
            batch: reservation.batch,
            before_descriptor: &descriptor_before,
            role: &storage.status_role().unwrap(),
        };
        let prepared = operation::prepare_init_in_population(
            &plan,
            &old,
            &mut frame,
            &mut allocator_after,
            &mut status_after,
            AllocationPopulation::Status,
        )
        .unwrap();
        write.preflight(&stamp).unwrap();
        let descriptor_after = prepared.descriptor_bytes();
        let allocator_after = prepared.allocator_after();
        let status_after = prepared.status_after();
        let requirement = prepared.stage_required(storage.wal()).unwrap();
        let mut key = write.retain_claim();
        if !completion {
            frame_init::corrupt_init_key_generation(&mut key);
        }
        let permit = InitInstallPermit {
            key: &key,
            allocator_before: stamp,
            requirement,
            allocator_after,
            status_after,
        };
        let mut phase = InitInstallPhase::StageOwned;
        let commit = InitControlCommit {
            descriptor: &mut reservation.writer.base.descriptor,
            after_descriptor: descriptor_after,
            phase: &mut phase,
        };
        let pins = frame_init::pin_state(reservation.writer.pool);
        let events = state.lock().unwrap().events.len();
        if completion {
            write.prepare_install(&permit, commit).unwrap().install();
            write.release_checked().unwrap();
            storage.wal().wait_durable(&requirement).unwrap();
            let before_finish = snapshot(reservation.writer.pool);
            assert!(before_finish[0].0.claim_present && !before_finish[0].0.initializer_pins_held);
            assert_eq!(before_finish[0].0.total, 0);
            assert_eq!(
                before_finish[1].0.published.unwrap().identity.page.page_no,
                2
            );
            // The current B key is valid; only the reported completion is stale.
            authority.generations[1] = 1;
            let completed = CompletedInitAuthority { authority };
            let events = state.lock().unwrap().events.len();
            let error = reservation
                .writer
                .pool
                .finish_init_claim(&mut key, &completed)
                .unwrap_err();
            assert_eq!(
                (error.cause, error.effect),
                (dirty_frame::Cause::Invariant, Effect::CallerMustResolve)
            );
            assert_eq!(snapshot(reservation.writer.pool), before_finish);
            assert_eq!(state.lock().unwrap().events.len(), events);
        } else {
            match write.prepare_install(&permit, commit) {
                Err(error) => assert_eq!(
                    (error.cause, error.effect),
                    (dirty_frame::Cause::Invariant, Effect::CallerMustResolve)
                ),
                Ok(_) => panic!("stale key must never authorize install"),
            }
            assert_eq!(frame_init::pin_state(reservation.writer.pool), pins);
            assert_eq!(reservation.writer.base.descriptor, descriptor_before);
            assert_eq!(phase, InitInstallPhase::StageOwned);
            assert_eq!(state.lock().unwrap().events.len(), events);
            drop(write);
            let after = snapshot(reservation.writer.pool);
            for i in 0..2 {
                assert_eq!(after[i].1, before[i].1);
                assert_eq!(after[i].0.page_lsn, before[i].0.page_lsn);
                assert_eq!(after[i].0.counter, before[i].0.counter);
                assert_eq!(after[i].0.entry, before[i].0.entry);
                assert_eq!(after[i].0.requirement, before[i].0.requirement);
                assert_eq!(after[i].0.published, before[i].0.published);
            }
            assert!(after[0].0.claim_present);
        }
        assert!(storage.session_fence().is_fenced());
        drop(key);
        drop(reservation);
        drop(bound);
    });
}

#[test]
fn operational_status_stale_install_key_cannot_publish_next_generation() {
    stale_staged_generation(false);
}

#[test]
fn operational_status_stale_completion_cannot_clear_next_claim() {
    stale_staged_generation(true);
}

#[test]
fn operational_status_producer_rejects_wrong_scope_before_writing() {
    use super::super::super::super::super::allocation_reservation::operation::{
        self, OperationBudget,
    };
    use crate::storage::allocation_page;
    fixture!(|original, core, state, storage| {
        let mut bound = converted(original.into_operational());
        let mut invalid = literal("work3-allocator.after");
        // Codec-valid RowLeaf cannot survive in the trusted Status population.
        invalid[10144 + 64 + 96 + 1] = 1;
        let checksum = foundation::codec::content_digest(&invalid[..PAGE_BYTES - 32]);
        invalid[PAGE_BYTES - 32..].copy_from_slice(&checksum);
        let page = allocation_page::decode_allocation_page(
            &invalid,
            bound.inner.inner.control.base.identity,
            storage.profile().spec().namespace,
        )
        .unwrap();
        assert_eq!(
            operation::validate_population(&page, None, AllocationPopulation::Status),
            Err(operation::SemanticError::PageMismatch)
        );
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let (mut writer, _, _) = bound.split_with_flush();
        let reservation = writer.reserve_one(&mut owner).unwrap();
        let before = snapshot(reservation.writer.pool);
        let events = state.lock().unwrap().events.len();
        let mut output = boxed::<65536>(0x73);
        let mut after = boxed::<PAGE_BYTES>(0x29);
        let old: [u8; PAGE_BYTES] = literal("work1-allocator.before").try_into().unwrap();
        for budget in [OperationBudget::Exclusive81920, OperationBudget::Dirty98304] {
            let plan = operation::Plan {
                profile: storage.profile(),
                identity: reservation.writer.base.identity,
                prefix: storage.wal().prefix().unwrap(),
                batch: reservation.batch,
                cancel: false,
                before_descriptor: None,
            };
            match operation::prepare_in_population(
                plan,
                &old,
                &mut output[..],
                &mut after,
                budget,
                AllocationPopulation::Status,
            ) {
                Err(error) => assert_eq!(error, operation::SemanticError::StateMismatch),
                Ok(_) => panic!("wrong trusted budget must not yield stage authority"),
            }
            assert!(output.iter().all(|b| *b == 0x73));
            assert!(after.iter().all(|b| *b == 0x29));
        }
        assert_eq!(snapshot(reservation.writer.pool), before);
        assert_eq!(state.lock().unwrap().events.len(), events);
        assert!(reservation.cancel().is_ok());
        assert!(!storage.session_fence().is_fenced());
        drop(bound);
    });
}
