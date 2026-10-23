//! New P3 paths only. The history writes actual A/B pages; reads use literal bytes.
use super::super::super::super::OPERATION_BYTES;
use super::super::tests::{HOME, boxed, converted, fixture, initialized, literal};
use super::*;
use crate::storage::{
    dirty_frame::{DirtyFrame, init_test_support as frame_init, status_load_test_support as probe},
    wal_io::{FreshStorageSession, bootstrap::test_support as port},
};
use crate::transaction::concurrency::{OwnerRole, PageConcurrency};

macro_rules! history {
    ($bound:ident, $owner:ident, $flusher:ident) => {{
        let first = {
            let (mut writer, _, mut flush) = $bound.split_with_flush();
            let first = initialized(writer.reserve_one(&mut $owner).unwrap());
            flush.try_flush_status(&mut $flusher).unwrap();
            flush.try_flush_allocator(&mut $flusher).unwrap();
            first
        };
        $bound.prepare_next_status().unwrap();
        let second = {
            let (mut writer, _, mut flush) = $bound.split_with_flush();
            assert!(writer.reserve_one(&mut $owner).unwrap().cancel().is_ok());
            let second = initialized(writer.reserve_one(&mut $owner).unwrap());
            flush.try_flush_status(&mut $flusher).unwrap();
            second
        };
        (first.page, second.page)
    }};
}
macro_rules! loaded {
    (|$bound:ident, $owner:ident, $flusher:ident, $state:ident, $storage:ident, $a:ident, $b:ident| $body:block) => {
        fixture!(|original, core, $state, $storage| {
            let mut $bound = converted(original.into_operational());
            let mut $owner = core.try_owner(OwnerRole::Maintenance).unwrap();
            let mut $flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
            let ($a, $b) = history!($bound, $owner, $flusher);
            $body
            drop($bound);
        });
    };
}

#[test]
fn status_load_resource_gate() {
    println!("status_load_call_bytes={}", call_bytes::<port::Writer>());
    println!(
        "snapshot={} permit={} result={}",
        size_of::<StatusLoadSnapshot<'_, '_>>(),
        size_of::<StatusLoadPermit<'_, '_, '_>>(),
        size_of::<Result<StatusLoadObservation>>()
    );
    println!(
        "dirty_and_flush_metadata={:?}",
        metadata_bytes::<port::Writer>()
    );
    assert!(fits::<port::Writer>());
}
#[test]
fn status_load_dirty_allocator_ab_and_empty() {
    loaded!(|bound, owner, flusher, state, storage, a, b| {
        let mut before = boxed::<PAGE_BYTES>(0);
        let mut prior = frame_init::snapshot(&bound.inner.inner.pool, 0, &mut before);
        assert!(prior.dirty);
        assert_eq!(prior.page_lsn.0, 6);
        assert_eq!(
            &state.lock().unwrap().files[HOME].bytes[..PAGE_BYTES],
            literal("work1-allocator.after")
        );
        let writes = state.lock().unwrap().events.len();
        for (page, name, generation) in [
            (a, "work1-status.after", 3),
            (b, "work3-status.after", 4),
            (a, "work1-status.after", 5),
        ] {
            let read_start = state.lock().unwrap().read_events.len();
            let observed = bound.load_status(page).unwrap();
            assert_eq!(observed.after.generation, generation);
            assert_eq!(observed.after.identity.page, page);
            assert_eq!(observed.after.rec_lsn, None);
            let (_, reader, _) = bound.split_with_flush();
            let read = reader.read(&mut owner).unwrap();
            assert_eq!(read.bytes().unwrap().as_slice(), literal(name));
            read.release_checked().unwrap();
            let s = state.lock().unwrap();
            let homes: Vec<_> = s.read_events[read_start..]
                .iter()
                .filter(|e| e.kind == "read" && e.name == HOME)
                .collect();
            assert_eq!(homes.len(), 1);
            assert_eq!(
                (homes[0].offset, homes[0].length),
                (page.page_no * PAGE_BYTES as u64, PAGE_BYTES)
            );
        }
        let mut after = boxed::<PAGE_BYTES>(0);
        let current = frame_init::snapshot(&bound.inner.inner.pool, 0, &mut after);
        assert_eq!(current.published.unwrap().identity.page, a);
        // RawSnapshot also includes the global payload publication. Only that
        // expected field changes; every allocator/pin/DPT/receipt field must match.
        prior.published = current.published;
        assert_eq!(current, prior);
        assert_eq!(before, after);
        assert_eq!(state.lock().unwrap().events.len(), writes);
        assert_eq!(
            bound.inner.inner.control.base.descriptor.as_slice(),
            literal("work3-terminal.descriptor")
        );
        assert_eq!(
            bound.prepare_next_status().unwrap_err().effect,
            Effect::Unchanged
        );
        bound
            .split_with_flush()
            .2
            .try_flush_allocator(&mut flusher)
            .unwrap();
        bound.prepare_next_status().unwrap();
        let observed = bound.load_status(b).unwrap();
        assert!(observed.before.is_none());
        assert_eq!(observed.after.generation, 7);
        assert!(!storage.session_fence().is_fenced());
    });
}
#[test]
fn status_load_bad_membership_and_resident_no_io() {
    loaded!(|bound, owner, flusher, state, storage, a, b| {
        let start = state.lock().unwrap().reads;
        let mut wrong = a;
        wrong.page_no = 3;
        let mut stale = a;
        stale.incarnation = b.incarnation;
        let mut foreign = a;
        foreign.file = crate::storage::page::FileId::new(0x200000001).unwrap();
        for page in [wrong, stale, foreign] {
            assert!(matches!(
                bound.load_status(page),
                Err(StatusLoadError {
                    cause: Cause::Membership,
                    effect: Effect::Unchanged
                })
            ));
        }
        assert!(matches!(
            bound.load_status(b),
            Err(StatusLoadError {
                cause: Cause::AlreadyResident,
                effect: Effect::Unchanged
            })
        ));
        assert_eq!(state.lock().unwrap().reads, start);
        assert!(!storage.session_fence().is_fenced());
    });
}
fn read_failure(at: usize) {
    loaded!(|bound, owner, flusher, state, storage, a, _b| {
        let mut old = boxed::<PAGE_BYTES>(0);
        let before = frame_init::snapshot(&bound.inner.inner.pool, 1, &mut old);
        let generations = bound.inner.inner.pool.init_generations().unwrap();
        let writes = state.lock().unwrap().events.len();
        {
            let mut s = state.lock().unwrap();
            s.read_plan = Some(port::Plan {
                at: s.reads + at,
                action: port::Action::BeforeError,
            });
        }
        let error = bound.load_status(a).unwrap_err();
        assert!(matches!(error.cause, Cause::PageIo(_)), "{error:?}");
        assert_eq!(error.effect, Effect::Unchanged);
        assert!(!storage.session_fence().is_fenced());
        let mut after = boxed::<PAGE_BYTES>(0);
        assert_eq!(
            frame_init::snapshot(&bound.inner.inner.pool, 1, &mut after),
            before
        );
        assert_eq!(after, old);
        assert_eq!(
            bound.inner.inner.pool.init_generations().unwrap(),
            generations
        );
        {
            let mut s = state.lock().unwrap();
            assert_eq!(s.configured_read_hits, 1);
            s.read_plan = None;
        }
        assert_eq!(bound.load_status(a).unwrap().after.generation, 3);
        assert_eq!(state.lock().unwrap().events.len(), writes);
    });
}
macro_rules! read_case {
    ($name:ident,$at:literal) => {
        #[test]
        fn $name() {
            read_failure($at);
        }
    };
}
read_case!(status_load_read_error_00, 0);
read_case!(status_load_read_error_01, 1);
read_case!(status_load_read_error_02, 2);
read_case!(status_load_read_error_03, 3);
read_case!(status_load_read_error_04, 4);
read_case!(status_load_read_error_05, 5);
read_case!(status_load_read_error_06, 6);
read_case!(status_load_read_error_07, 7);
read_case!(status_load_read_error_08, 8);
read_case!(status_load_read_error_09, 9);

#[test]
fn status_load_corrupt_allocator_and_requirement() {
    for variant in 0..3 {
        loaded!(|bound, owner, flusher, state, storage, a, _b| {
            match variant {
                0 => probe::corrupt_allocator(&bound.inner.inner.pool),
                1 => probe::corrupt_requirement(&bound.inner.inner.pool),
                _ => bound.inner.inner.control.base.descriptor[0] ^= 1,
            }
            let start = state.lock().unwrap().reads;
            assert_eq!(
                bound.load_status(a).unwrap_err().effect,
                Effect::CallerMustResolve
            );
            assert!(storage.session_fence().is_fenced());
            assert_eq!(state.lock().unwrap().reads, start);
        });
    }
}
#[test]
fn status_load_generation_exhaustion_and_dirty_payload() {
    for variant in 0..2 {
        loaded!(|bound, owner, flusher, state, storage, a, _b| {
            bound
                .split_with_flush()
                .2
                .try_flush_allocator(&mut flusher)
                .unwrap();
            if variant == 0 {
                frame_init::seed_status_generation(&mut bound.inner.inner.pool, u64::MAX);
            } else {
                bound.prepare_next_status().unwrap();
                let (mut writer, _, _) = bound.split_with_flush();
                initialized(writer.reserve_one(&mut owner).unwrap());
            }
            let start = state.lock().unwrap().reads;
            assert_eq!(bound.load_status(a).unwrap_err().effect, Effect::Unchanged);
            assert!(!storage.session_fence().is_fenced());
            assert_eq!(state.lock().unwrap().reads, start);
        });
    }
}
#[test]
fn status_load_home_corruption_and_unwind() {
    for variant in 0..2 {
        loaded!(|bound, owner, flusher, state, storage, a, _b| {
            let mut prior = boxed::<PAGE_BYTES>(0);
            let old = frame_init::snapshot(&bound.inner.inner.pool, 1, &mut prior);
            {
                let mut s = state.lock().unwrap();
                if variant == 0 {
                    s.files.get_mut(HOME).unwrap().bytes[PAGE_BYTES + 100] ^= 1;
                } else {
                    s.read_plan = Some(port::Plan {
                        at: s.reads + 9,
                        action: port::Action::BeforePanic,
                    });
                }
            }
            if variant == 0 {
                assert_eq!(
                    bound.load_status(a).unwrap_err().effect,
                    Effect::CallerMustResolve
                );
            } else {
                assert!(
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| bound.load_status(a)))
                        .is_err()
                );
            }
            assert!(storage.session_fence().is_fenced());
            let mut after = boxed::<PAGE_BYTES>(0);
            assert_eq!(
                frame_init::snapshot(&bound.inner.inner.pool, 1, &mut after),
                old
            );
            assert_eq!(prior, after);
        });
    }
}
#[test]
fn status_load_stale_and_foreign_permit() {
    for variant in 0..2 {
        loaded!(|bound, owner, flusher, state, storage, _a, _b| {
            let mut old_bytes = boxed::<PAGE_BYTES>(0);
            let old = frame_init::snapshot(&bound.inner.inner.pool, 1, &mut old_bytes);
            let generation = probe::generation(&bound.inner.inner.pool);
            let mut before = boxed::<PAGE_BYTES>(0);
            let mut snapshot = bound
                .inner
                .inner
                .pool
                .prepare_status_load(&mut before)
                .unwrap();
            static FOREIGN: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
            if variant == 0 {
                probe::stale(&mut snapshot);
            } else {
                probe::foreign(&mut snapshot, &FOREIGN);
            }
            let request = bound
                .inner
                .inner
                .pool
                .initialized_request()
                .unwrap()
                .unwrap();
            let bytes = boxed::<PAGE_BYTES>(0);
            let permit = StatusLoadPermit {
                snapshot: &snapshot,
                allocator: &before,
                bytes: &bytes,
                request,
                lsn: PageLsn(2),
            };
            assert_eq!(
                bound
                    .inner
                    .inner
                    .pool
                    .install_status_load(permit)
                    .unwrap_err()
                    .effect,
                Effect::CallerMustResolve
            );
            assert!(storage.session_fence().is_fenced());
            let mut new_bytes = boxed::<PAGE_BYTES>(0);
            assert_eq!(
                frame_init::snapshot(&bound.inner.inner.pool, 1, &mut new_bytes),
                old
            );
            assert_eq!(old_bytes, new_bytes);
            assert_eq!(probe::generation(&bound.inner.inner.pool), generation);
        });
    }
}

#[test]
fn status_load_typed_and_lsn_corruption() {
    for name in ["typed", "zero_lsn", "future_lsn"] {
        loaded!(|bound, owner, flusher, state, storage, a, _b| {
            let bad = std::fs::read(format!(
                "{}/../../tests/fixtures/dev3-status-load/{name}.page",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap();
            state.lock().unwrap().files.get_mut(HOME).unwrap().bytes[PAGE_BYTES..2 * PAGE_BYTES]
                .copy_from_slice(&bad);
            let error = bound.load_status(a).unwrap_err();
            assert!(matches!(error.cause, Cause::Semantic(_) | Cause::Invariant));
            assert_eq!(error.effect, Effect::CallerMustResolve);
            assert!(storage.session_fence().is_fenced());
        });
    }
}
#[test]
fn status_load_leaked_pin_and_flush_pending() {
    for variant in 0..2 {
        loaded!(|bound, owner, flusher, state, storage, a, _b| {
            if variant == 0 {
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
            } else {
                bound.inner.flush.phase = super::super::super::Phase::Captured;
            }
            let start = state.lock().unwrap().reads;
            assert_eq!(bound.load_status(a).unwrap_err().effect, Effect::Unchanged);
            assert!(!storage.session_fence().is_fenced());
            assert_eq!(state.lock().unwrap().reads, start);
        });
    }
}
#[test]
fn status_load_selection_corruption() {
    loaded!(|bound, owner, flusher, state, storage, a, _b| {
        state
            .lock()
            .unwrap()
            .files
            .get_mut("FILEMAP")
            .unwrap()
            .bytes[100] ^= 1;
        let error = bound.load_status(a).unwrap_err();
        assert!(matches!(error.cause, Cause::PageIo(_)));
        assert_eq!(error.effect, Effect::CallerMustResolve);
        assert!(storage.session_fence().is_fenced());
    });
}

fn s5_history(profile: u32, rounds: usize, fault: Option<(usize, bool)>) {
    fixture!(profile, |original, core, state, storage| {
        let mut bound = converted(original.into_operational());
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let (a, b) = history!(bound, owner, flusher);
        if let Some((at, panic)) = fault {
            {
                let mut s = state.lock().unwrap();
                s.read_plan = Some(port::Plan {
                    at: s.reads + at,
                    action: if panic {
                        port::Action::BeforePanic
                    } else {
                        port::Action::BeforeError
                    },
                });
            }
            if panic {
                assert!(
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| bound.load_status(a)))
                        .is_err()
                );
                assert!(storage.session_fence().is_fenced());
            } else {
                assert_eq!(bound.load_status(a).unwrap_err().effect, Effect::Unchanged);
                assert!(!storage.session_fence().is_fenced());
            }
            assert_eq!(state.lock().unwrap().configured_read_hits, 1);
        } else {
            for round in 0..rounds {
                for (i, page, name) in [(0, a, "work1-status.after"), (1, b, "work3-status.after")]
                {
                    assert_eq!(
                        bound.load_status(page).unwrap().after.generation,
                        3 + (round * 2 + i) as u64
                    );
                    let (_, reader, _) = bound.split_with_flush();
                    let guard = reader.read(&mut owner).unwrap();
                    let expected = std::fs::read(format!(
                        "{}/../../tests/fixtures/dev3-operational-status/p{profile}/{name}",
                        env!("CARGO_MANIFEST_DIR")
                    ))
                    .unwrap();
                    assert_eq!(guard.bytes().unwrap().as_slice(), expected);
                    guard.release_checked().unwrap();
                }
            }
        }
        drop(bound);
    });
}
#[test]
#[ignore = "DEV3 final S5 only: profile2 actual selected load"]
fn status_load_s5_profile2() {
    s5_history(2, 1, None);
}
#[test]
#[ignore = "DEV3 final S5 only: profile3 actual selected load"]
fn status_load_s5_profile3() {
    s5_history(3, 1, None);
}
#[test]
#[ignore = "DEV3 final S5 only: 32 A/B rounds, 64 actual home loads"]
fn status_load_s5_rounds() {
    s5_history(1, 32, None);
}
macro_rules! s5_fault {
    ($name:ident,$profile:literal,$at:literal,$panic:literal) => {
        #[test]
        #[ignore = "DEV3 final S5 only: expanded profile/read/fault cell"]
        fn $name() {
            s5_history($profile, 0, Some(($at, $panic)));
        }
    };
}
s5_fault!(status_load_s5_p2_00_error, 2, 0, false);
s5_fault!(status_load_s5_p2_00_panic, 2, 0, true);
s5_fault!(status_load_s5_p2_01_error, 2, 1, false);
s5_fault!(status_load_s5_p2_01_panic, 2, 1, true);
s5_fault!(status_load_s5_p2_02_error, 2, 2, false);
s5_fault!(status_load_s5_p2_02_panic, 2, 2, true);
s5_fault!(status_load_s5_p2_03_error, 2, 3, false);
s5_fault!(status_load_s5_p2_03_panic, 2, 3, true);
s5_fault!(status_load_s5_p2_04_error, 2, 4, false);
s5_fault!(status_load_s5_p2_04_panic, 2, 4, true);
s5_fault!(status_load_s5_p2_05_error, 2, 5, false);
s5_fault!(status_load_s5_p2_05_panic, 2, 5, true);
s5_fault!(status_load_s5_p2_06_error, 2, 6, false);
s5_fault!(status_load_s5_p2_06_panic, 2, 6, true);
s5_fault!(status_load_s5_p2_07_error, 2, 7, false);
s5_fault!(status_load_s5_p2_07_panic, 2, 7, true);
s5_fault!(status_load_s5_p2_08_error, 2, 8, false);
s5_fault!(status_load_s5_p2_08_panic, 2, 8, true);
s5_fault!(status_load_s5_p2_09_error, 2, 9, false);
s5_fault!(status_load_s5_p2_09_panic, 2, 9, true);
s5_fault!(status_load_s5_p3_00_error, 3, 0, false);
s5_fault!(status_load_s5_p3_00_panic, 3, 0, true);
s5_fault!(status_load_s5_p3_01_error, 3, 1, false);
s5_fault!(status_load_s5_p3_01_panic, 3, 1, true);
s5_fault!(status_load_s5_p3_02_error, 3, 2, false);
s5_fault!(status_load_s5_p3_02_panic, 3, 2, true);
s5_fault!(status_load_s5_p3_03_error, 3, 3, false);
s5_fault!(status_load_s5_p3_03_panic, 3, 3, true);
s5_fault!(status_load_s5_p3_04_error, 3, 4, false);
s5_fault!(status_load_s5_p3_04_panic, 3, 4, true);
s5_fault!(status_load_s5_p3_05_error, 3, 5, false);
s5_fault!(status_load_s5_p3_05_panic, 3, 5, true);
s5_fault!(status_load_s5_p3_06_error, 3, 6, false);
s5_fault!(status_load_s5_p3_06_panic, 3, 6, true);
s5_fault!(status_load_s5_p3_07_error, 3, 7, false);
s5_fault!(status_load_s5_p3_07_panic, 3, 7, true);
s5_fault!(status_load_s5_p3_08_error, 3, 8, false);
s5_fault!(status_load_s5_p3_08_panic, 3, 8, true);
s5_fault!(status_load_s5_p3_09_error, 3, 9, false);
s5_fault!(status_load_s5_p3_09_panic, 3, 9, true);

#[test]
fn status_load_fence_during_read_keeps_first_cause() {
    loaded!(|bound, owner, flusher, state, storage, a, _b| {
        let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
        {
            let mut s = state.lock().unwrap();
            let at = s.reads + 9;
            s.read_plan = Some(port::Plan {
                at,
                action: port::Action::BeforeError,
            });
            s.read_pause = Some(port::Pause {
                at,
                after: false,
                entered: entered_tx,
                release: release_rx,
                unwind: false,
            });
        }
        std::thread::scope(|scope| {
            let fence = storage.session_fence();
            let worker = scope.spawn(move || {
                entered_rx
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap();
                fence.fence();
                release_tx.send(()).unwrap();
            });
            let error = bound.load_status(a).unwrap_err();
            assert!(
                matches!(error.cause, Cause::PageIo(PageIoError::Read(_))),
                "{error:?}"
            );
            assert_eq!(error.effect, Effect::CallerMustResolve);
            worker.join().unwrap();
        });
        assert!(storage.session_fence().is_fenced());
    });
}
#[test]
fn status_load_then_prepare_and_next_init() {
    loaded!(|bound, owner, flusher, state, storage, a, _b| {
        let loaded = bound.load_status(a).unwrap();
        bound
            .split_with_flush()
            .2
            .try_flush_allocator(&mut flusher)
            .unwrap();
        assert_eq!(bound.prepare_next_status().unwrap(), loaded.after);
        let (mut writer, reader, mut flush) = bound.split_with_flush();
        let next = initialized(writer.reserve_one(&mut owner).unwrap());
        assert_eq!(
            (
                next.page.page_no,
                next.page.incarnation.get(),
                next.lsn.0,
                next.status.generation
            ),
            (3, 2, 8, 4)
        );
        let guard = reader.read(&mut owner).unwrap();
        assert_eq!(guard.stamps().unwrap().1, next.status);
        guard.release_checked().unwrap();
        flush.try_flush_status(&mut flusher).unwrap();
        flush.try_flush_allocator(&mut flusher).unwrap();
        assert!(!storage.session_fence().is_fenced());
    });
}
#[test]
fn status_load_active_reservation_and_leaked_init_claim() {
    for claim in [false, true] {
        loaded!(|bound, owner, flusher, state, storage, a, _b| {
            bound
                .split_with_flush()
                .2
                .try_flush_allocator(&mut flusher)
                .unwrap();
            bound.prepare_next_status().unwrap();
            let (mut writer, _, _) = bound.split_with_flush();
            let reservation = writer.reserve_one(&mut owner).unwrap();
            if claim {
                let authority = reservation
                    .writer
                    .authority(reservation.batch, reservation.page, reservation.ordinal)
                    .unwrap();
                let pins = reservation
                    .writer
                    .pool
                    .claim_init_targets(reservation.owner, &authority)
                    .unwrap();
                std::mem::forget(pins);
            }
            std::mem::forget(reservation);
            let start = state.lock().unwrap().reads;
            assert!(matches!(
                bound.load_status(a),
                Err(StatusLoadError {
                    cause: Cause::Busy,
                    effect: Effect::Unchanged
                })
            ));
            if claim {
                let mut scratch = boxed::<PAGE_BYTES>(0);
                assert!(matches!(
                    bound.inner.inner.pool.prepare_status_load(&mut scratch),
                    Err(dirty_frame::DirtyError {
                        cause: dirty_frame::Cause::Busy,
                        effect: Effect::Unchanged
                    })
                ));
            }
            assert_eq!(state.lock().unwrap().reads, start);
            assert!(!storage.session_fence().is_fenced());
        });
    }
}
#[test]
fn status_load_global_flush_claim_is_busy() {
    loaded!(|bound, owner, flusher, state, storage, a, _b| {
        let mut scratch = boxed::<PAGE_BYTES>(0);
        let request = bound.inner.inner.pool.target_request(0).unwrap();
        let capture = bound
            .inner
            .inner
            .pool
            .capture_flush(&mut flusher, request, &mut scratch)
            .unwrap()
            .unwrap();
        let start = state.lock().unwrap().reads;
        assert!(matches!(
            bound.load_status(a),
            Err(StatusLoadError {
                cause: Cause::Frame(dirty_frame::DirtyError {
                    cause: dirty_frame::Cause::Busy,
                    ..
                }),
                effect: Effect::Unchanged
            })
        ));
        std::hint::black_box(capture);
        assert_eq!(state.lock().unwrap().reads, start);
        assert!(!storage.session_fence().is_fenced());
    });
}

#[test]
fn status_load_selected_read_schedule() {
    loaded!(|bound, owner, flusher, state, storage, a, _b| {
        let start = state.lock().unwrap().read_events.len();
        bound.load_status(a).unwrap();
        // Authored from PageFileReader/open_existing_with_metadata: metadata
        // belongs to each open call, and is not an extra injectable event.
        let expected = [
            ("open", "d3-profile", 0, 0),
            ("read", "d3-profile", 0, 4096),
            ("open", "FILEMAP", 0, 0),
            ("read", "FILEMAP", 0, 4096),
            ("open", "d3-filemap-0", 0, 0),
            ("read", "d3-filemap-0", 0, 320),
            ("read", "d3-filemap-0", 16384, 256),
            ("read", "d3-filemap-0", 16384, 256),
            ("open", HOME, 0, 0),
            ("read", HOME, 16384, 16384),
        ];
        let s = state.lock().unwrap();
        let actual: Vec<_> = s.read_events[start..]
            .iter()
            .map(|e| (e.kind, e.name.as_str(), e.offset, e.length))
            .collect();
        assert_eq!(actual, expected);
        assert!(!storage.session_fence().is_fenced());
    });
}
