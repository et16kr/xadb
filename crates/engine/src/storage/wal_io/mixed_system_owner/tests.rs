//! New mixed-owner scenarios only. Setup runs the real owner; expected wire
//! images are the independent mixed INIT literals, never product encoders.
use super::*;
use crate::{
    storage::{
        dirty_frame::{
            self, DirtyFrame, SharedPool, init_test_support as init_probe,
            pressure_test_support as pressure_probe, status_load_test_support as load_probe,
        },
        wal_io::{self as wal, FreshStorageSession, bootstrap::test_support as port},
    },
    transaction::concurrency::{OwnerRole, PageConcurrency},
};
use foundation::codec::content_digest;
use std::{
    mem::size_of,
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

const HOME: &str = "d3-data-0000000100000001";
const INIT_OPERATION_BYTES: usize = super::super::super::OPERATION_BYTES;
type State = Arc<Mutex<port::State>>;
type SlotSnapshot = (
    init_probe::RawSnapshot,
    Vec<u8>,
    Option<(TargetRequest, u64)>,
);

fn boxed<const N: usize>(byte: u8) -> Box<[u8; N]> {
    vec![byte; N].into_boxed_slice().try_into().unwrap()
}
fn literal(profile: u32, name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../../tests/fixtures/dev3-mixed-system-init/p{profile}/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}
fn err<T, E>(value: std::result::Result<T, E>) -> E {
    match value {
        Err(error) => error,
        Ok(_) => panic!("unexpected mixed-owner success"),
    }
}
fn initialized<W: WriterNamespace>(
    reservation: ReservedSystemPage<'_, '_, '_, '_, '_, '_, '_, '_, '_, W>,
) -> InitializedSystemObservation {
    match reservation.initialize() {
        Ok(value) => value,
        Err(InitAttemptError::Before(error)) => panic!("INIT before {:?}", error.error),
        Err(InitAttemptError::Fatal(error)) => panic!("INIT fatal {error:?}"),
    }
}
fn cancelled<W: WriterNamespace>(
    reservation: ReservedSystemPage<'_, '_, '_, '_, '_, '_, '_, '_, '_, W>,
) {
    match reservation.cancel() {
        Ok(_) => {}
        Err(InitAttemptError::Before(error)) => panic!("cancel before {:?}", error.error),
        Err(InitAttemptError::Fatal(error)) => panic!("cancel fatal {error:?}"),
    }
}
fn run_case(id: &str, run: impl FnOnce()) {
    let included = std::env::var("XADB_MIXED_OWNER_INCLUDE")
        .map_or(true, |value| value.split(',').any(|part| part == id));
    let excluded = std::env::var("XADB_MIXED_OWNER_EXCLUDE")
        .is_ok_and(|value| value.split(',').any(|part| part == id));
    if !included || excluded {
        println!("MIXED_OWNER_CASE_SKIP\t{id}");
        return;
    }
    println!("MIXED_OWNER_CASE_BEGIN\t{id}");
    run();
    println!("MIXED_OWNER_CASE_PASS\t{id}");
}
fn snapshot(pool: &SharedPool<'_, '_, '_, '_>) -> Vec<SlotSnapshot> {
    (0..3)
        .map(|index| {
            let mut bytes = boxed::<PAGE_BYTES>(0);
            let mut metadata = init_probe::snapshot(pool, index, &mut bytes);
            // The old diagnostic's publication is selected-slot global. Keep
            // every per-slot registration independently in the third element.
            metadata.published = None;
            (metadata, bytes.to_vec(), load_probe::slot(pool, index))
        })
        .collect()
}
fn io_counts(state: &State) -> (usize, usize) {
    let state = state.lock().unwrap();
    (state.events.len(), state.reads)
}
fn file_inventory(state: &State) -> Vec<(String, u64, [u8; 32])> {
    state
        .lock()
        .unwrap()
        .files
        .iter()
        .map(|(name, entry)| {
            assert!(entry.sparse_tail.is_empty());
            (name.clone(), entry.length, content_digest(&entry.bytes))
        })
        .collect()
}
macro_rules! fixture {
    ($profile:expr, |$bound:ident, $core:ident, $state:ident, $storage:ident| $body:block) => {{
        let mut wal_slot = port::slot();
        let mut workspace = boxed::<{ 4 << 20 }>(0);
        let (namespace, $state) = port::fresh();
        let fresh = FreshStorageSession::create_undo(
            namespace, $profile, &mut wal_slot, &mut [[0; PAGE_BYTES]; 3],
        ).unwrap();
        let installed = fresh.install_doublewrite(&mut workspace).unwrap();
        let $storage = installed.into_mutable_storage();
        let $core = PageConcurrency::new($storage.session_fence()).unwrap();
        let mut allocator = boxed::<PAGE_BYTES>(0xa1);
        let mut first = boxed::<PAGE_BYTES>(0xb2);
        let mut second = boxed::<PAGE_BYTES>(0xc3);
        let mut operation = boxed::<INIT_OPERATION_BYTES>(0x73);
        let mut frames = [
            DirtyFrame::new(&mut allocator), DirtyFrame::new(&mut first),
            DirtyFrame::new(&mut second),
        ];
        let binding = $storage.bind_mixed_system(&$core, &mut frames, &mut operation).unwrap();
        let mut $bound = match binding.activate() {
            Ok(bound) => bound,
            Err(IntoFlushError::Before { error, .. }) | Err(IntoFlushError::Fatal(error)) => {
                panic!("mixed activation {error:?}")
            }
        };
        $body
        let before_drop = io_counts(&$state);
        drop($bound);
        assert!($storage.session_fence().is_fenced());
        assert_eq!(io_counts(&$state), before_drop);
        drop($storage);
        let state = $state.lock().unwrap();
        assert_eq!((state.handles, state.writers), (0, 0));
    }};
}
macro_rules! two_dirty {
    ($bound:ident, $owner:ident) => {{
        let a = {
            let (mut writer, _, _) = $bound.split();
            initialized(writer.reserve(&mut $owner, SystemPageKind::Status).unwrap())
        };
        let mut cursor = SystemResidencyCursor::default();
        assert_eq!(
            $bound.prepare_next(&mut cursor).unwrap(),
            SystemSlotStep::Prepared {
                index: 2,
                previous: None,
            }
        );
        let b = {
            let (mut writer, _, _) = $bound.split();
            initialized(
                writer
                    .reserve(&mut $owner, SystemPageKind::UndoRoot)
                    .unwrap(),
            )
        };
        (a, b)
    }};
}
macro_rules! three_dirty_history {
    ($bound:ident, $owner:ident, $flusher:ident) => {{
        let (a, b) = two_dirty!($bound, $owner);
        let mut cursor = SystemResidencyCursor::default();
        assert_eq!(
            $bound.prepare_next(&mut cursor).unwrap(),
            SystemSlotStep::FlushRequired {
                target: a.target,
                rec_lsn: PageLsn(2),
            }
        );
        assert!($bound.split().2.try_flush(&mut $flusher, a.target).is_ok());
        assert_eq!(
            $bound.prepare_next(&mut cursor).unwrap(),
            SystemSlotStep::Prepared {
                index: 1,
                previous: Some(FrameStamp {
                    rec_lsn: None,
                    ..a.payload
                }),
            }
        );
        let c = {
            let (mut writer, _, _) = $bound.split();
            initialized(
                writer
                    .reserve(&mut $owner, SystemPageKind::UndoRoot)
                    .unwrap(),
            )
        };
        (a, b, c)
    }};
}
fn assert_work(
    bound: &MixedSystem<'_, '_, '_, '_, '_, '_, '_, port::Writer>,
    state: &State,
    profile: u32,
    work: u32,
) {
    let current = snapshot(&bound.inner.inner.pool);
    assert_eq!(
        current[0].1,
        literal(profile, &format!("sequence/work{work}-allocator.after"))
    );
    assert_eq!(
        bound.inner.inner.control.base.descriptor.as_slice(),
        literal(profile, &format!("sequence/work{work}-terminal.descriptor"))
    );
    let expected: Vec<_> = (1..=work)
        .flat_map(|part| {
            let mut bytes = literal(profile, &format!("sequence/work{part}-reserve.wal"));
            bytes.extend(literal(
                profile,
                &format!("sequence/work{part}-terminal.wal"),
            ));
            bytes
        })
        .collect();
    assert_eq!(
        &state.lock().unwrap().files[wal::WAL_NAME].bytes[4096..],
        expected
    );
}
fn lifecycle(profile: u32) {
    fixture!(profile, |bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let (a, b, c) = three_dirty_history!(bound, owner, flusher);
        assert_eq!(
            (
                a.target.page.page_no,
                b.target.page.page_no,
                c.target.page.page_no
            ),
            (1, 2, 3)
        );
        assert_eq!((a.lsn.0, b.lsn.0, c.lsn.0), (2, 4, 6));
        assert_eq!(
            (
                a.payload.generation,
                b.payload.generation,
                c.payload.generation
            ),
            (1, 1, 2)
        );
        assert_work(&bound, &state, profile, 3);
        let current = snapshot(&bound.inner.inner.pool);
        assert_eq!(
            current[1].1,
            literal(profile, "sequence/work3-payload.after")
        );
        assert_eq!(
            current[2].1,
            literal(profile, "sequence/work2-payload.after")
        );
        assert_eq!(
            current
                .iter()
                .map(|entry| entry.0.rec_lsn.unwrap().0)
                .collect::<Vec<_>>(),
            [1, 6, 4]
        );
        assert!(
            current
                .iter()
                .all(|entry| entry.0.dirty && entry.0.total == 0)
        );
        assert_eq!(current[1].2.unwrap().0.identity.owner.get(), 3);
        assert_eq!(current[2].2.unwrap().0.kind, PageKind::Undo);
        let state = state.lock().unwrap();
        assert_eq!(
            &state.files[HOME].bytes[PAGE_BYTES..2 * PAGE_BYTES],
            literal(profile, "sequence/work1-payload.after")
        );
        assert_eq!(
            &state.files[HOME].bytes[..PAGE_BYTES],
            literal(profile, "allocator.genesis")
        );
        assert!(
            state.files[HOME].bytes[2 * PAGE_BYTES..4 * PAGE_BYTES]
                .iter()
                .all(|byte| *byte == 0)
        );
        assert!(c.wal.synced_lsn >= 6);
        assert!(!storage.session_fence().is_fenced());
    });
}
#[test]
fn mixed_owner_three_frames_status_undo_pressure_continuation() {
    run_case("MO-LIVE-001", || lifecycle(1));
}
#[test]
#[ignore = "DEV3 final S5 only; new mixed profile expansion"]
fn mixed_owner_s5_profile2_three_frame_lifecycle() {
    run_case("MO-S5-LIVE-P2", || lifecycle(2));
}
#[test]
#[ignore = "DEV3 final S5 only; new mixed profile expansion"]
fn mixed_owner_s5_profile3_three_frame_lifecycle() {
    run_case("MO-S5-LIVE-P3", || lifecycle(3));
}

fn typed_read(kind: SystemPageKind) {
    fixture!(1, |bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let (a, b) = two_dirty!(bound, owner);
        let (target, work) = if kind == SystemPageKind::Status {
            (a.target, 1)
        } else {
            (b.target, 2)
        };
        let before = snapshot(&bound.inner.inner.pool);
        let counts = io_counts(&state);
        {
            let (_, reader, _) = bound.split();
            let guard = reader.read(&mut owner, target).unwrap();
            assert_eq!(
                guard.bytes().unwrap().as_slice(),
                literal(1, &format!("sequence/work{work}-payload.after"))
            );
            let (allocator, payload) = guard.stamps().unwrap();
            assert_eq!(allocator.page_lsn, PageLsn(4));
            assert_eq!(payload.identity.page, target.page);
            assert_eq!(payload.page_lsn, PageLsn(2 * work as u64));
            guard.release_checked().unwrap();
        }
        assert_eq!(snapshot(&bound.inner.inner.pool), before);
        assert_eq!(io_counts(&state), counts);
        assert!(!storage.session_fence().is_fenced());
    });
}
#[test]
fn mixed_owner_status_typed_read_full_bytes_and_release() {
    run_case("MO-READ-001", || typed_read(SystemPageKind::Status));
}
#[test]
fn mixed_owner_undo_typed_read_full_bytes_and_release() {
    run_case("MO-READ-002", || typed_read(SystemPageKind::UndoRoot));
}
fn wrong_read(variant: u8) {
    fixture!(1, |bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let (a, b) = two_dirty!(bound, owner);
        let mut target = if variant == 1 { b.target } else { a.target };
        match variant {
            0 => target.kind = SystemPageKind::UndoRoot,
            1 => target.kind = SystemPageKind::Status,
            2 => target.page.page_no = 3,
            3 => target.page.incarnation = foundation::identity::Generation::new(3).unwrap(),
            _ => unreachable!(),
        }
        let before = snapshot(&bound.inner.inner.pool);
        let counts = io_counts(&state);
        assert_eq!(
            err(bound.split().1.read(&mut owner, target)).effect,
            Effect::Unchanged
        );
        assert_eq!(snapshot(&bound.inner.inner.pool), before);
        assert_eq!(io_counts(&state), counts);
        assert!(!storage.session_fence().is_fenced());
    });
}
macro_rules! scenario {
    ($name:ident, $id:literal, $body:expr) => {
        #[test]
        fn $name() {
            run_case($id, || $body);
        }
    };
}
scenario!(
    mixed_owner_status_cannot_be_read_as_undo,
    "MO-READ-003",
    wrong_read(0)
);
scenario!(
    mixed_owner_undo_cannot_be_read_as_status,
    "MO-READ-004",
    wrong_read(1)
);
scenario!(
    mixed_owner_absent_page_read_has_no_effect,
    "MO-READ-005",
    wrong_read(2)
);
scenario!(
    mixed_owner_stale_incarnation_read_has_no_effect,
    "MO-READ-006",
    wrong_read(3)
);

#[test]
fn mixed_owner_cancel_preserves_prior_mixed_pages() {
    run_case("MO-CANCEL-001", || {
        fixture!(1, |bound, core, state, storage| {
            let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
            let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
            let (_, b, _) = three_dirty_history!(bound, owner, flusher);
            bound.split().2.try_flush(&mut flusher, b.target).unwrap();
            let mut cursor = SystemResidencyCursor::default();
            assert!(matches!(
                bound.prepare_next(&mut cursor).unwrap(),
                SystemSlotStep::Prepared { index: 2, .. }
            ));
            let before = snapshot(&bound.inner.inner.pool);
            {
                let (mut writer, _, _) = bound.split();
                let reserved = writer
                    .reserve(&mut owner, SystemPageKind::UndoRoot)
                    .unwrap();
                assert_eq!(reserved.target().page.page_no, 4);
                cancelled(reserved);
            }
            let after = snapshot(&bound.inner.inner.pool);
            assert_eq!(after[0].1, literal(1, "cancel/work4-allocator.after"));
            assert_eq!(&after[1..], &before[1..]);
            assert_eq!(
                bound.inner.inner.control.base.descriptor.as_slice(),
                literal(1, "cancel/work4-terminal.descriptor")
            );
            assert_eq!(
                &state.lock().unwrap().files[wal::WAL_NAME].bytes[4096..],
                literal(1, "all.frames-only")
            );
            assert!(!storage.session_fence().is_fenced());
        });
    });
}
fn drop_reservation(kind: SystemPageKind) {
    fixture!(1, |bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let (mut writer, _, _) = bound.split();
        let reserved = writer.reserve(&mut owner, kind).unwrap();
        let counts = io_counts(&state);
        drop(reserved);
        assert!(storage.session_fence().is_fenced());
        assert_eq!(io_counts(&state), counts);
        assert_eq!(
            &state.lock().unwrap().files[wal::WAL_NAME].bytes[4096..],
            literal(1, "sequence/work1-reserve.wal")
        );
    });
}
scenario!(
    mixed_owner_dropped_status_reservation_fences,
    "MO-DROP-001",
    drop_reservation(SystemPageKind::Status)
);
scenario!(
    mixed_owner_dropped_undo_reservation_fences,
    "MO-DROP-002",
    drop_reservation(SystemPageKind::UndoRoot)
);

#[test]
fn mixed_owner_prepared_empty_slot_preserves_bytes_and_generation() {
    run_case("MO-PRESSURE-001", || {
        fixture!(1, |bound, core, state, storage| {
            let before = snapshot(&bound.inner.inner.pool);
            let counts = io_counts(&state);
            let mut cursor = SystemResidencyCursor::default();
            assert_eq!(
                bound.prepare_next(&mut cursor).unwrap(),
                SystemSlotStep::Prepared {
                    index: 1,
                    previous: None
                }
            );
            assert_eq!(cursor.0.unwrap().get(), 2);
            assert_eq!(snapshot(&bound.inner.inner.pool), before);
            assert_eq!(io_counts(&state), counts);
            assert!(!storage.session_fence().is_fenced());
        });
    });
}
fn pressure(variant: u8) {
    fixture!(1, |bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let (a, b) = two_dirty!(bound, owner);
        let mut cursor = SystemResidencyCursor::default();
        match variant {
            1 => {
                pressure_probe::conservative_rec_lsn(&bound.inner.inner.pool, 2, PageLsn(2));
                cursor.0 = std::num::NonZeroUsize::new(2);
            }
            2 => pressure_probe::forget_content_write(&bound.inner.inner.pool, 2),
            3 => pressure_probe::duplicate_registration(&mut bound.inner.inner.pool),
            4 => {
                pressure_probe::generation(&mut bound.inner.inner.pool, 1, u64::MAX);
                pressure_probe::generation(&mut bound.inner.inner.pool, 2, u64::MAX);
            }
            5 => dirty_frame::test_support::poison_content(&bound.inner.inner.pool, 2),
            6 => dirty_frame::test_support::seed_counter(&bound.inner.inner.pool, 2, 0),
            _ => {}
        }
        let counts = io_counts(&state);
        let files = file_inventory(&state);
        let old_cursor = cursor.0;
        if variant <= 1 {
            let before = snapshot(&bound.inner.inner.pool);
            assert_eq!(
                bound.prepare_next(&mut cursor).unwrap(),
                SystemSlotStep::FlushRequired {
                    target: if variant == 0 { a.target } else { b.target },
                    rec_lsn: PageLsn(2),
                }
            );
            assert_eq!(snapshot(&bound.inner.inner.pool), before);
        } else {
            let error = bound.prepare_next(&mut cursor).unwrap_err();
            let fatal = matches!(variant, 3 | 5 | 6);
            assert_eq!(
                error.effect,
                if fatal {
                    Effect::CallerMustResolve
                } else {
                    Effect::Unchanged
                }
            );
            assert!(matches!(error.cause, Cause::Frame(_)), "{error:?}");
            assert_eq!(storage.session_fence().is_fenced(), fatal);
        }
        assert_eq!(cursor.0, old_cursor);
        assert_eq!(io_counts(&state), counts);
        assert_eq!(file_inventory(&state), files);
    });
}
scenario!(
    mixed_owner_pressure_oldest_dirty_keeps_original_slots,
    "MO-PRESSURE-002",
    pressure(0)
);
scenario!(
    mixed_owner_pressure_dirty_tie_selects_cursor_undo,
    "MO-PRESSURE-003",
    pressure(1)
);
scenario!(
    mixed_owner_pressure_late_busy_cannot_hide_behind_status,
    "MO-PRESSURE-004",
    pressure(2)
);
scenario!(
    mixed_owner_pressure_late_duplicate_registration_fences,
    "MO-PRESSURE-005",
    pressure(3)
);
scenario!(
    mixed_owner_pressure_generation_exhaustion_has_no_effect,
    "MO-PRESSURE-006",
    pressure(4)
);
scenario!(
    mixed_owner_pressure_late_undo_poison_fences,
    "MO-PRESSURE-007",
    pressure(5)
);
scenario!(
    mixed_owner_pressure_late_undo_counter_corruption_fences,
    "MO-PRESSURE-008",
    pressure(6)
);

fn flush(kind: SystemPageKind, fault: bool) {
    fixture!(1, |bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let (a, b) = two_dirty!(bound, owner);
        let (target, index, work) = if kind == SystemPageKind::Status {
            (a.target, 1, 1)
        } else {
            (b.target, 2, 2)
        };
        let before = snapshot(&bound.inner.inner.pool);
        let start = state.lock().unwrap().events.len();
        let original_files = file_inventory(&state);
        if fault {
            state.lock().unwrap().plan = Some(port::Plan {
                at: start,
                action: port::Action::BeforeError,
            });
        }
        let result = bound.split().2.try_flush(&mut dedicated, target);
        if fault {
            let error = result.unwrap_err();
            assert_eq!(error.effect, Effect::CallerMustResolve);
            assert!(storage.session_fence().is_fenced());
            assert_eq!(file_inventory(&state), original_files);
            let after = snapshot(&bound.inner.inner.pool);
            assert_eq!(after[index].1, before[index].1);
            assert!(after[index].0.dirty && after[index].0.entry.is_some());
            let (hit, count, name, offset, length, kind) = {
                let state = state.lock().unwrap();
                let event = &state.events[start];
                (
                    state.hit,
                    state.events.len(),
                    event.name.clone(),
                    event.offset,
                    event.length,
                    event.kind,
                )
            };
            assert_eq!(hit, 1);
            assert_eq!(count, start + 1);
            assert_eq!(
                (name.as_str(), offset, length, kind),
                ("d3-dwb-stage", 0, PAGE_BYTES, "write")
            );
        } else {
            let observed = match result.unwrap() {
                FlushProgress::Complete(observed) => observed,
                other => panic!("expected full mixed flush {other:?}"),
            };
            assert!(observed.finalized.cleaned);
            assert_eq!((observed.io.attempted, observed.io.completed), (31, 31));
            assert!(observed.io.homes_synced && observed.io.retirement_selected);
            assert_eq!(
                (observed.io.last_issued, observed.io.known.generation),
                (1, 4)
            );
            let after = snapshot(&bound.inner.inner.pool);
            assert!(!after[index].0.dirty);
            assert!(after[index].0.entry.is_none() && after[index].0.requirement.is_none());
            assert_eq!(after[index].1, before[index].1);
            assert_eq!(after[0], before[0]);
            assert_eq!(after[3 - index], before[3 - index]);
            let state = state.lock().unwrap();
            assert_eq!(state.events.len(), start + 31);
            let home_writes: Vec<_> = state.events[start..]
                .iter()
                .filter(|event| event.kind == "write" && event.name == HOME)
                .collect();
            assert_eq!(home_writes.len(), 1);
            assert_eq!(
                home_writes[0].offset,
                target.page.page_no * PAGE_BYTES as u64
            );
            assert_eq!(
                home_writes[0].data,
                literal(1, &format!("sequence/work{work}-payload.after"))
            );
            let offset = target.page.page_no as usize * PAGE_BYTES;
            assert_eq!(
                &state.files[HOME].bytes[offset..offset + PAGE_BYTES],
                home_writes[0].data
            );
            assert!(!storage.session_fence().is_fenced());
        }
    });
}
scenario!(
    mixed_owner_status_flush_writes_exact_home_and_retires,
    "MO-FLUSH-001",
    flush(SystemPageKind::Status, false)
);
scenario!(
    mixed_owner_undo_flush_writes_exact_home_and_retires,
    "MO-FLUSH-002",
    flush(SystemPageKind::UndoRoot, false)
);
scenario!(
    mixed_owner_status_first_dwb_error_keeps_dirty_obligation,
    "MO-FLUSH-003",
    flush(SystemPageKind::Status, true)
);
scenario!(
    mixed_owner_undo_first_dwb_error_keeps_dirty_obligation,
    "MO-FLUSH-004",
    flush(SystemPageKind::UndoRoot, true)
);

#[test]
fn mixed_owner_layout_observations() {
    run_case("MO-LAYOUT-001", || {
        type W = port::Writer;
        println!(
            "MIXED_OWNER_LAYOUT binding={} owner={} writer={} reader={} flusher={} reservation={} init_result={} slot_step={} cursor={}",
            size_of::<MixedSystemBinding<'_, '_, '_, '_, '_, '_, '_, W>>(),
            size_of::<MixedSystem<'_, '_, '_, '_, '_, '_, '_, W>>(),
            size_of::<MixedSystemWriter<'_, '_, '_, '_, '_, '_, '_, '_, W>>(),
            size_of::<MixedSystemReader<'_, '_, '_, '_, '_>>(),
            size_of::<MixedSystemFlusher<'_, '_, '_, '_, '_, '_, '_, W>>(),
            size_of::<ReservedSystemPage<'_, '_, '_, '_, '_, '_, '_, '_, '_, W>>(),
            size_of::<
                std::result::Result<
                    InitializedSystemObservation,
                    InitAttemptError<ReservedSystemPage<'_, '_, '_, '_, '_, '_, '_, '_, '_, W>>,
                >,
            >(),
            size_of::<SystemSlotStep>(),
            size_of::<SystemResidencyCursor>()
        );
        assert_eq!(
            size_of::<MixedSystem<'_, '_, '_, '_, '_, '_, '_, W>>(),
            size_of::<BoundStatusWithFlush<'_, '_, '_, '_, '_, '_, '_, W>>()
        );
        // Complete call-phase/resource admission is a separate root-owned gate;
        // equality of retained wrappers alone does not prove that gate.
    });
}

#[test]
fn mixed_owner_load_exact_status_and_undo_keeps_dirty_allocator() {
    run_case("MO-LOAD-001", || {
        fixture!(1, |bound, core, state, storage| {
            let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
            let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
            let (a, b, c) = three_dirty_history!(bound, owner, flusher);
            bound.split().2.try_flush(&mut flusher, b.target).unwrap();
            let before = snapshot(&bound.inner.inner.pool);
            let effect_count = state.lock().unwrap().events.len();
            let loaded = bound.load_into(2, a.target).unwrap();
            assert_eq!(loaded.before.unwrap().identity.page, b.target.page);
            assert_eq!(
                (
                    loaded.after.identity.page,
                    loaded.after.generation,
                    loaded.after.rec_lsn
                ),
                (a.target.page, 2, None)
            );
            let after = snapshot(&bound.inner.inner.pool);
            assert_eq!(after[0], before[0]);
            assert_eq!(after[1], before[1]);
            assert_eq!(after[2].1, literal(1, "sequence/work1-payload.after"));
            assert_eq!(state.lock().unwrap().events.len(), effect_count);
            // B is now absent and its exact home is on disk. Reuse the clean A
            // destination without touching dirty C or the dirty allocator.
            let loaded = bound.load_into(2, b.target).unwrap();
            assert_eq!(loaded.before.unwrap().identity.page, a.target.page);
            assert_eq!(
                (
                    loaded.after.identity.page,
                    loaded.after.generation,
                    loaded.after.rec_lsn
                ),
                (b.target.page, 3, None)
            );
            let last = snapshot(&bound.inner.inner.pool);
            assert_eq!(last[0], before[0]);
            assert_eq!(last[1], before[1]);
            assert_eq!(last[2].1, literal(1, "sequence/work2-payload.after"));
            assert_eq!(state.lock().unwrap().events.len(), effect_count);
            assert_work(&bound, &state, 1, 3);
            let (_, reader, _) = bound.split();
            let guard = reader.read(&mut owner, c.target).unwrap();
            assert_eq!(
                guard.bytes().unwrap().as_slice(),
                literal(1, "sequence/work3-payload.after")
            );
            guard.release_checked().unwrap();
            assert!(!storage.session_fence().is_fenced());
        });
    });
}
#[test]
fn mixed_owner_residency_hit_and_flush_candidate_preserve_cursor_rules() {
    run_case("MO-LOAD-002", || {
        fixture!(1, |bound, core, state, storage| {
            let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
            let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
            let (a, b, c) = three_dirty_history!(bound, owner, flusher);
            let before = snapshot(&bound.inner.inner.pool);
            let counts = io_counts(&state);
            let mut cursor = SystemResidencyCursor::default();
            assert_eq!(
                bound.residency_step(b.target, &mut cursor).unwrap(),
                SystemResidencyStep::Resident {
                    index: 2,
                    frame: b.payload,
                    loaded: false
                }
            );
            assert_eq!(cursor.0.unwrap().get(), 1);
            assert_eq!(
                bound.residency_step(a.target, &mut cursor).unwrap(),
                SystemResidencyStep::FlushRequired {
                    target: b.target,
                    rec_lsn: PageLsn(4)
                }
            );
            assert_eq!(cursor.0.unwrap().get(), 1);
            assert_eq!(snapshot(&bound.inner.inner.pool), before);
            assert_eq!(io_counts(&state), counts);
            bound.split().2.try_flush(&mut flusher, b.target).unwrap();
            assert!(
                matches!(bound.residency_step(a.target, &mut cursor).unwrap(), SystemResidencyStep::Resident { index: 2, loaded: true, frame } if frame.identity.page == a.target.page && frame.generation == 2)
            );
            assert_eq!(cursor.0.unwrap().get(), 1);
            assert_eq!(
                snapshot(&bound.inner.inner.pool)[1].1,
                literal(1, "sequence/work3-payload.after")
            );
            assert_eq!(c.target.kind, SystemPageKind::UndoRoot);
            assert!(!storage.session_fence().is_fenced());
        });
    });
}
fn load_membership(variant: u8) {
    fixture!(1, |bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let (a, b, c) = three_dirty_history!(bound, owner, flusher);
        bound.split().2.try_flush(&mut flusher, b.target).unwrap();
        let mut target = a.target;
        let mut index = 2;
        match variant {
            0 => target.kind = SystemPageKind::UndoRoot,
            1 => target.page.page_no = 4,
            2 => target.page.incarnation = foundation::identity::Generation::new(3).unwrap(),
            3 => target = c.target,
            4 => index = 0,
            _ => unreachable!(),
        }
        let before = snapshot(&bound.inner.inner.pool);
        let counts = io_counts(&state);
        assert_eq!(
            bound.load_into(index, target).unwrap_err().effect,
            Effect::Unchanged
        );
        assert_eq!(snapshot(&bound.inner.inner.pool), before);
        assert_eq!(io_counts(&state), counts);
        assert!(!storage.session_fence().is_fenced());
    });
}
scenario!(
    mixed_owner_load_wrong_class_is_no_effect,
    "MO-LOAD-003",
    load_membership(0)
);
scenario!(
    mixed_owner_load_absent_member_is_no_effect,
    "MO-LOAD-004",
    load_membership(1)
);
scenario!(
    mixed_owner_load_stale_member_is_no_effect,
    "MO-LOAD-005",
    load_membership(2)
);
scenario!(
    mixed_owner_load_resident_address_is_no_effect,
    "MO-LOAD-006",
    load_membership(3)
);
scenario!(
    mixed_owner_load_cannot_replace_allocator,
    "MO-LOAD-007",
    load_membership(4)
);

fn load_read_fault(wal_source: bool, action: port::Action, fatal: bool) {
    fixture!(1, |bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let (a, b, _) = three_dirty_history!(bound, owner, flusher);
        bound.split().2.try_flush(&mut flusher, b.target).unwrap();
        let before = snapshot(&bound.inner.inner.pool);
        let writes = state.lock().unwrap().events.len();
        let files = file_inventory(&state);
        state.lock().unwrap().named_read_plan = Some(if wal_source {
            (wal::WAL_NAME, 4096, 160, action)
        } else {
            (HOME, PAGE_BYTES as u64, PAGE_BYTES, action)
        });
        let error = bound.load_into(2, a.target).unwrap_err();
        assert_eq!(
            error.effect,
            if fatal {
                Effect::CallerMustResolve
            } else {
                Effect::Unchanged
            }
        );
        assert_eq!(storage.session_fence().is_fenced(), fatal);
        assert_eq!(snapshot(&bound.inner.inner.pool), before);
        assert_eq!(state.lock().unwrap().events.len(), writes);
        assert_eq!(state.lock().unwrap().configured_read_hits, 1);
        assert_eq!(file_inventory(&state), files);
        if !fatal {
            let loaded = bound.load_into(2, a.target).unwrap();
            assert_eq!(loaded.after.generation, 2);
            assert_eq!(
                snapshot(&bound.inner.inner.pool)[2].1,
                literal(1, "sequence/work1-payload.after")
            );
            assert_eq!(state.lock().unwrap().events.len(), writes);
        }
    });
}
scenario!(
    mixed_owner_load_home_before_error_preserves_generation_then_retries,
    "MO-LOAD-008",
    load_read_fault(false, port::Action::BeforeError, false)
);
scenario!(
    mixed_owner_load_wal_before_error_preserves_generation_then_retries,
    "MO-LOAD-009",
    load_read_fault(true, port::Action::BeforeError, false)
);
scenario!(
    mixed_owner_load_required_wal_eof_fences,
    "MO-LOAD-010",
    load_read_fault(true, port::Action::Zero, true)
);
scenario!(
    mixed_owner_load_required_wal_prefix_eof_fences,
    "MO-LOAD-011",
    load_read_fault(true, port::Action::PrefixEof, true)
);
scenario!(
    mixed_owner_load_required_wal_invalid_count_fences,
    "MO-LOAD-012",
    load_read_fault(true, port::Action::InvalidCount, true)
);
scenario!(
    mixed_owner_load_home_invalid_count_fences,
    "MO-LOAD-013",
    load_read_fault(false, port::Action::InvalidCount, true)
);

fn bad_home(variant: u8) {
    fixture!(1, |bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let (a, b, _) = three_dirty_history!(bound, owner, flusher);
        bound.split().2.try_flush(&mut flusher, b.target).unwrap();
        let target = if variant == 0 {
            a.target
        } else {
            bound.load_into(2, a.target).unwrap();
            b.target
        };
        let mut wrong = literal(
            1,
            if variant == 0 {
                "sequence/work1-payload.after"
            } else {
                "sequence/work2-payload.after"
            },
        );
        match variant {
            0 => wrong[64..72].copy_from_slice(&1u64.to_le_bytes()),
            1 | 2 => {
                let record = std::fs::read(format!(
                    "{}/../../tests/fixtures/dev3-undo-free-page/{}",
                    env!("CARGO_MANIFEST_DIR"),
                    if variant == 1 {
                        "free-g6.bin"
                    } else {
                        "free-gmax.bin"
                    }
                ))
                .unwrap();
                assert_eq!(record.len(), 432);
                wrong[15920..16352].copy_from_slice(&record);
            }
            3 => {
                wrong = literal(1, "sequence/work3-payload.after");
                wrong[32..40].copy_from_slice(&2u64.to_le_bytes());
            }
            4 => wrong = literal(1, "sequence/work1-payload.after"),
            5 => {
                let record = std::fs::read(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../tests/fixtures/dev3-undo-free-page/existing-U02-live.bin"
                ))
                .unwrap();
                assert_eq!(record.len(), 432);
                wrong[15920..16352].copy_from_slice(&record);
            }
            6 => {
                let chunk = historical_chunk();
                let start = 16352 - chunk.len();
                wrong[128..16352].fill(0);
                wrong[72..74].copy_from_slice(&1u16.to_le_bytes());
                wrong[74..76].copy_from_slice(&132u16.to_le_bytes());
                wrong[76..78].copy_from_slice(&(start as u16).to_le_bytes());
                wrong[128..130].copy_from_slice(&(start as u16).to_le_bytes());
                wrong[130..132].copy_from_slice(&(chunk.len() as u16).to_le_bytes());
                wrong[start..16352].copy_from_slice(&chunk);
            }
            _ => unreachable!(),
        }
        let digest = content_digest(&wrong[..16352]);
        wrong[16352..].copy_from_slice(&digest);
        let offset = target.page.page_no as usize * PAGE_BYTES;
        state.lock().unwrap().files.get_mut(HOME).unwrap().bytes[offset..offset + PAGE_BYTES]
            .copy_from_slice(&wrong);
        let before = snapshot(&bound.inner.inner.pool);
        let writes = state.lock().unwrap().events.len();
        let files = file_inventory(&state);
        let error = bound.load_into(2, target).unwrap_err();
        assert_eq!(error.effect, Effect::CallerMustResolve);
        assert!(storage.session_fence().is_fenced());
        assert_eq!(snapshot(&bound.inner.inner.pool), before);
        assert_eq!(state.lock().unwrap().events.len(), writes);
        assert_eq!(file_inventory(&state), files);
    });
}
scenario!(
    mixed_owner_load_rehashed_old_status_lsn_is_not_original_init,
    "MO-LOAD-014",
    bad_home(0)
);
scenario!(
    mixed_owner_load_rehashed_undo_free_generation6_is_rejected,
    "MO-LOAD-015",
    bad_home(1)
);
scenario!(
    mixed_owner_load_rehashed_undo_free_generation_max_is_rejected,
    "MO-LOAD-016",
    bad_home(2)
);
scenario!(
    mixed_owner_load_rehashed_undo_c_image_cannot_replace_b,
    "MO-LOAD-017",
    bad_home(3)
);
scenario!(
    mixed_owner_load_status_home_cannot_replace_undo,
    "MO-LOAD-018",
    bad_home(4)
);

fn binding_scope(plan3: bool) {
    let mut wal_slot = port::slot();
    let mut workspace = boxed::<{ 4 << 20 }>(0);
    let (namespace, state) = port::fresh();
    let fresh = if plan3 {
        FreshStorageSession::create_undo(namespace, 1, &mut wal_slot, &mut [[0; PAGE_BYTES]; 3])
    } else {
        FreshStorageSession::create_status(namespace, 1, &mut wal_slot, &mut [[0; PAGE_BYTES]; 3])
    }
    .unwrap();
    let installed = fresh.install_doublewrite(&mut workspace).unwrap();
    let storage = installed.into_mutable_storage();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut allocator = boxed::<PAGE_BYTES>(0xa1);
    let mut first = boxed::<PAGE_BYTES>(0xb2);
    let mut second = boxed::<PAGE_BYTES>(0xc3);
    let mut operation = boxed::<INIT_OPERATION_BYTES>(0x73);
    let mut frames = [
        DirtyFrame::new(&mut allocator),
        DirtyFrame::new(&mut first),
        DirtyFrame::new(&mut second),
    ];
    let counts = io_counts(&state);
    if plan3 {
        assert_eq!(
            err(storage.bind_resident_status(&core, &mut frames, &mut operation)).effect,
            Effect::Unchanged
        );
        assert_eq!(io_counts(&state), counts);
        let binding = storage
            .bind_mixed_system(&core, &mut frames, &mut operation)
            .unwrap();
        drop(binding);
    } else {
        assert_eq!(
            err(storage.bind_mixed_system(&core, &mut frames, &mut operation)).effect,
            Effect::Unchanged
        );
        assert_eq!(io_counts(&state), counts);
        let binding = storage
            .bind_resident_status(&core, &mut frames, &mut operation)
            .unwrap();
        drop(binding);
    }
    assert!(storage.session_fence().is_fenced());
    drop(storage);
    let state = state.lock().unwrap();
    assert_eq!((state.handles, state.writers), (0, 0));
}
scenario!(
    mixed_owner_plan2_refusal_preserves_original_status_binding,
    "MO-BIND-001",
    binding_scope(false)
);
scenario!(
    mixed_owner_plan3_refuses_old_status_before_mixed_binding,
    "MO-BIND-002",
    binding_scope(true)
);

fn foreign_owner(fenced: bool) {
    let foreign_fence = crate::storage::session_fence::SessionFence::new();
    let foreign_core = PageConcurrency::new(&foreign_fence).unwrap();
    fixture!(1, |bound, core, state, storage| {
        let mut foreign = foreign_core.try_owner(OwnerRole::Maintenance).unwrap();
        let before = snapshot(&bound.inner.inner.pool);
        let counts = io_counts(&state);
        if fenced {
            storage.session_fence().fence();
        }
        let error = err(bound
            .split()
            .0
            .reserve(&mut foreign, SystemPageKind::UndoRoot));
        assert_eq!(
            error.effect,
            if fenced {
                Effect::CallerMustResolve
            } else {
                Effect::Unchanged
            }
        );
        assert_eq!(storage.session_fence().is_fenced(), fenced);
        assert_eq!(snapshot(&bound.inner.inner.pool), before);
        assert_eq!(io_counts(&state), counts);
    });
}
scenario!(
    mixed_owner_foreign_reserver_has_no_effect,
    "MO-ORIGIN-001",
    foreign_owner(false)
);
scenario!(
    mixed_owner_health_precedes_foreign_reserver,
    "MO-ORIGIN-002",
    foreign_owner(true)
);

#[cfg(target_os = "linux")]
#[path = "tests/native.rs"]
mod native;

// Historical independent U19 chunk literal SHA-256 2221bcbac40ed59d5849c11eeb1b05376747052b581af53bed838a49473ea77e.
// This is a chunk-bearing canonical page-envelope negative, not root authority.
fn historical_chunk() -> Vec<u8> {
    const HEX: &str = "58554331010000000100000001080000010800000000000002000000010000001e000000000000000400000000000000000000000000000006000000000000006400000000000000020000000100000028000000000000000400000000000000000000000000000000000000000000000000000000000000bff27d226a616eb526eea24d35af880abab1456476cca73dd28b33205a832ddd11000000000000000100000004f8070000031425364758697a8b9cadbecfe0f102132435465768798a9bacbdcedff00112233445566778899aabbccddeef00112233445566778899aabbccddeeff102132435465768798a9bacbdcedfe0f2031425364758697a8b9cadbecfd0e1f30415263748596a7b8c9daebfc0d1e2f405162738495a6b7c8d9eafb0c1d2e3f5061728394a5b6c7d8e9fa0b1c2d3e4f60718293a4b5c6d7e8f90a1b2c3d4e5f708192a3b4c5d6e7f8091a2b3c4d5e6f8091a2b3c4d5e6f708192a3b4c5d6e7f90a1b2c3d4e5f60718293a4b5c6d7e8fa0b1c2d3e4f5061728394a5b6c7d8e9fb0c1d2e3f405162738495a6b7c8d9eafc0d1e2f30415263748596a7b8c9daebfd0e1f2031425364758697a8b9cadbecfe0f102132435465768798a9bacbdcedff00112233445566778899aabbccddeef00112233445566778899aabbccddeeff102132435465768798a9bacbdcedfe0f2031425364758697a8b9cadbecfd0e1f30415263748596a7b8c9daebfc0d1e2f405162738495a6b7c8d9eafb0c1d2e3f5061728394a5b6c7d8e9fa0b1c2d3e4f60718293a4b5c6d7e8f90a1b2c3d4e5f708192a3b4c5d6e7f8091a2b3c4d5e6f8091a2b3c4d5e6f708192a3b4c5d6e7f90a1b2c3d4e5f60718293a4b5c6d7e8fa0b1c2d3e4f5061728394a5b6c7d8e9fb0c1d2e3f405162738495a6b7c8d9eafc0d1e2f30415263748596a7b8c9daebfd0e1f2031425364758697a8b9cadbecfe0f102132435465768798a9bacbdcedff00112233445566778899aabbccddeef00112233445566778899aabbccddeeff102132435465768798a9bacbdcedfe0f2031425364758697a8b9cadbecfd0e1f30415263748596a7b8c9daebfc0d1e2f405162738495a6b7c8d9eafb0c1d2e3f5061728394a5b6c7d8e9fa0b1c2d3e4f60718293a4b5c6d7e8f90a1b2c3d4e5f708192a3b4c5d6e7f8091a2b3c4d5e6f8091a2b3c4d5e6f708192a3b4c5d6e7f90a1b2c3d4e5f60718293a4b5c6d7e8fa0b1c2d3e4f5061728394a5b6c7d8e9fb0c1d2e3f405162738495a6b7c8d9eafc0d1e2f30415263748596a7b8c9daebfd0e1f2031425364758697a8b9cadbecfe0f102132435465768798a9bacbdcedff00112233445566778899aabbccddeef00112233445566778899aabbccddeeff102132435465768798a9bacbdcedfe0f2031425364758697a8b9cadbecfd0e1f30415263748596a7b8c9daebfc0d1e2f405162738495a6b7c8d9eafb0c1d2e3f5061728394a5b6c7d8e9fa0b1c2d3e4f60718293a4b5c6d7e8f90a1b2c3d4e5f708192a3b4c5d6e7f8091a2b3c4d5e6f8091a2b3c4d5e6f708192a3b4c5d6e7f90a1b2c3d4e5f60718293a4b5c6d7e8fa0b1c2d3e4f5061728394a5b6c7d8e9fb0c1d2e3f405162738495a6b7c8d9eafc0d1e2f30415263748596a7b8c9daebfd0e1f2031425364758697a8b9cadbecfe0f102132435465768798a9bacbdcedff00112233445566778899aabbccddeef00112233445566778899aabbccddeeff102132435465768798a9bacbdcedfe0f2031425364758697a8b9cadbecfd0e1f30415263748596a7b8c9daebfc0d1e2f405162738495a6b7c8d9eafb0c1d2e3f5061728394a5b6c7d8e9fa0b1c2d3e4f60718293a4b5c6d7e8f90a1b2c3d4e5f708192a3b4c5d6e7f8091a2b3c4d5e6f8091a2b3c4d5e6f708192a3b4c5d6e7f90a1b2c3d4e5f60718293a4b5c6d7e8fa0b1c2d3e4f5061728394a5b6c7d8e9fb0c1d2e3f405162738495a6b7c8d9eafc0d1e2f30415263748596a7b8c9daebfd0e1f2031425364758697a8b9cadbecfe0f102132435465768798a9bacbdcedff00112233445566778899aabbccddeef00112233445566778899aabbccddeeff102132435465768798a9bacbdcedfe0f2031425364758697a8b9cadbecfd0e1f30415263748596a7b8c9daebfc0d1e2f405162738495a6b7c8d9eafb0c1d2e3f5061728394a5b6c7d8e9fa0b1c2d3e4f60718293a4b5c6d7e8f90a1b2c3d4e5f708192a3b4c5d6e7f8091a2b3c4d5e6f8091a2b3c4d5e6f708192a3b4c5d6e7f90a1b2c3d4e5f60718293a4b5c6d7e8fa0b1c2d3e4f5061728394a5b6c7d8e9fb0c1d2e3f405162738495a6b7c8d9eafc0d1e2f30415263748596a7b8c9daebfd0e1f2031425364758697a8b9cadbecfe0f102132435465768798a9bacbdcedff00112233445566778899aabbccddeef00112233445566778899aabbccddeeff102132435465768798a9bacbdcedfe0f2031425364758697a8b9cadbecfd0e1f30415263748596a7b8c9daebfc0d1e2f405162738495a6b7c8d9eafb0c1d2e3f5061728394a5b6c7d8e9fa0b1c2d3e4f60718293a4b5c6d7e8f90a1b2c3d4e5f708192a3b4c5d6e7f8091a2b3c4d5e6f8091a2b3c4d5e6f708192a3b4c5d6e7f90a1b2c3d4e5f60718293a4b5c6d7e8fa0b1c2d3e4f5061728394a5b6c7d8e9fb0c1d2e3f405162738495a6b7c8d9eafc0d1e2f30415263748596a7b8c9daebfd0e1f2031425364758697a8b9cadbecfe0f102132435465768798a9bacbdcedff00112233445566778899aabbccddeef00112233445566778899aabbccddeeff102132435465768798a9bacbdcedfe0f2031425364758697a8b9cadbecfd0e1f30415263748596a7b8c9daebfc0d1e2f405162738495a6b7c8d9eafb0c1d2e3f5061728394a5b6c7d8e9fa0b1c2d3e4f60718293a4b5c6d7e8f90a1b2c3d4e5f708192a3b4c5d6e7f8091a2b3c4d5e6f8091a2b3c4d5e6f708192a3b4c5d6e7f90a1b2c3d4e5f60718293a4b5c6d7e8fa0b1c2d3e4f5061728394a5b6c7d8e9fb0c1d2e3f405162738495a6b7c8d9eafc0d1e2f30415263748596a";
    HEX.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
scenario!(
    mixed_owner_load_occupied_undo_root_is_rejected,
    "MO-LOAD-019",
    bad_home(5)
);
scenario!(
    mixed_owner_load_chunk_bearing_undo_envelope_is_rejected,
    "MO-LOAD-020",
    bad_home(6)
);

fn final_home_drift(undo: bool) {
    fixture!(1, |bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let (a, b, _) = three_dirty_history!(bound, owner, flusher);
        bound.split().2.try_flush(&mut flusher, b.target).unwrap();
        let target = if undo {
            bound.load_into(2, a.target).unwrap();
            b.target
        } else {
            a.target
        };
        let before = snapshot(&bound.inner.inner.pool);
        let writes = state.lock().unwrap().events.len();
        let start = state.lock().unwrap().reads;
        // Frozen source count, without a calibration run: each original home
        // reader performs ten port read/open events; history has one epoch
        // plus two reads per each of six frames. The final home is event32.
        let final_read = start + 10 + 1 + 6 * 2 + 9;
        let offset = target.page.page_no as usize * PAGE_BYTES;
        let mut changed = state.lock().unwrap().files[HOME].bytes.clone();
        changed[offset + 64..offset + 72].copy_from_slice(&1u64.to_le_bytes());
        let hash = content_digest(&changed[offset..offset + 16352]);
        changed[offset + 16352..offset + PAGE_BYTES].copy_from_slice(&hash);
        let expected = changed[offset..offset + PAGE_BYTES].to_vec();
        state
            .lock()
            .unwrap()
            .read_edits
            .push((final_read, HOME.into(), changed));
        let mut cursor = SystemResidencyCursor::default();
        assert_eq!(
            bound
                .residency_step(target, &mut cursor)
                .unwrap_err()
                .effect,
            Effect::CallerMustResolve
        );
        assert!(storage.session_fence().is_fenced());
        assert!(cursor.0.is_none());
        assert_eq!(snapshot(&bound.inner.inner.pool), before);
        let state = state.lock().unwrap();
        assert_eq!(state.events.len(), writes);
        assert!(state.read_edits.is_empty());
        let event = &state.read_events[final_read];
        assert_eq!(
            (event.kind, event.name.as_str(), event.offset, event.length),
            ("read", HOME, offset as u64, PAGE_BYTES)
        );
        assert_eq!(event.data, expected);
        assert_eq!(state.reads, final_read + 1);
    });
}
scenario!(
    mixed_owner_final_status_home_drift_fences_without_install,
    "MO-LOAD-021",
    final_home_drift(false)
);
scenario!(
    mixed_owner_final_undo_home_drift_fences_without_install,
    "MO-LOAD-022",
    final_home_drift(true)
);

macro_rules! prepare_class {
    ($bound:ident, $owner:ident, $kind:expr) => {{
        if $kind == SystemPageKind::UndoRoot {
            {
                let (mut writer, _, _) = $bound.split();
                initialized(writer.reserve(&mut $owner, SystemPageKind::Status).unwrap());
            }
            let mut cursor = SystemResidencyCursor::default();
            assert_eq!(
                $bound.prepare_next(&mut cursor).unwrap(),
                SystemSlotStep::Prepared {
                    index: 2,
                    previous: None
                }
            );
            2
        } else {
            1
        }
    }};
}
fn init_before_failure(kind: SystemPageKind) {
    fixture!(1, |bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let work = prepare_class!(bound, owner, kind);
        let (mut writer, _, _) = bound.split();
        let mut reservation = writer.reserve(&mut owner, kind).unwrap();
        let target = reservation.target();
        let before = snapshot(reservation.inner.writer.pool);
        let files = file_inventory(&state);
        let counts = io_counts(&state);
        reservation.inner.ordinal = 1;
        let mut reservation = match reservation.initialize() {
            Err(InitAttemptError::Before(failure)) => {
                assert_eq!(failure.error.effect, Effect::Unchanged);
                failure.reservation
            }
            _ => panic!("unchanged refusal must retain the exact linear reservation"),
        };
        assert_eq!(reservation.target(), target);
        assert_eq!(snapshot(reservation.inner.writer.pool), before);
        assert_eq!(file_inventory(&state), files);
        assert_eq!(io_counts(&state), counts);
        assert!(!storage.session_fence().is_fenced());
        reservation.inner.ordinal = 0;
        let observed = initialized(reservation);
        assert_eq!(observed.target, target);
        assert_eq!(observed.lsn, PageLsn(2 * work));
        let current = snapshot(writer.inner.pool);
        assert_eq!(
            current[work as usize].1,
            literal(1, &format!("sequence/work{work}-payload.after"))
        );
        assert_eq!(
            writer.inner.base.descriptor.as_slice(),
            literal(1, &format!("sequence/work{work}-terminal.descriptor"))
        );
    });
}
scenario!(
    mixed_owner_status_init_before_failure_retains_exact_reservation,
    "MO-INIT-BEFORE-S",
    init_before_failure(SystemPageKind::Status)
);
scenario!(
    mixed_owner_undo_init_before_failure_retains_exact_reservation,
    "MO-INIT-BEFORE-U",
    init_before_failure(SystemPageKind::UndoRoot)
);

const INIT_CUTS: [init_probe::InitPoint; 8] = [
    init_probe::InitPoint::StageOwned,
    init_probe::InitPoint::AllocatorImageInstalled,
    init_probe::InitPoint::PayloadImageInstalled,
    init_probe::InitPoint::MetadataInstalled,
    init_probe::InitPoint::RegistryInstalled,
    init_probe::InitPoint::Published,
    init_probe::InitPoint::DptGuardOwnedBeforePrecheck,
    init_probe::InitPoint::DptPrecheckedBeforePrepared,
];
fn init_partial(kind: SystemPageKind, cut: usize) {
    fixture!(1, |bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let work = prepare_class!(bound, owner, kind);
        let (mut writer, _, _) = bound.split();
        let reservation = writer.reserve(&mut owner, kind).unwrap();
        let pool = reservation.inner.writer.pool;
        let before = snapshot(pool);
        let descriptor = reservation.inner.writer.base.descriptor.to_vec();
        let wal_before = state.lock().unwrap().files[wal::WAL_NAME].bytes.clone();
        let files = file_inventory(&state);
        let counts = io_counts(&state);
        let (arrived, arrival) = mpsc::sync_channel(1);
        let (resume, release) = mpsc::sync_channel(1);
        init_probe::install_init_pause(pool, INIT_CUTS[cut], arrived, release);
        let fence = storage.session_fence();
        std::thread::scope(|scope| {
            let controller = scope.spawn(move || {
                arrival.recv_timeout(Duration::from_secs(5)).unwrap();
                assert!(!fence.is_fenced());
                assert_eq!(init_probe::pin_state(pool).1, 2);
                assert!(dirty_frame::test_support::content_busy(pool, 0));
                assert!(dirty_frame::test_support::content_busy(pool, work as usize));
                resume
                    .send(dirty_frame::test_support::Resume::Panic)
                    .unwrap();
            });
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _ = reservation.initialize();
                }))
                .is_err()
            );
            controller.join().unwrap();
        });
        assert!(storage.session_fence().is_fenced());
        let after = snapshot(writer.inner.pool);
        let allocator_installed = (1..=5).contains(&cut);
        let payload_installed = (2..=5).contains(&cut);
        let metadata_installed = (3..=5).contains(&cut);
        let registry_installed = (4..=5).contains(&cut);
        let published = cut == 5;
        assert_eq!(
            after[0].1,
            if allocator_installed {
                literal(1, &format!("sequence/work{work}-allocator.after"))
            } else {
                before[0].1.clone()
            }
        );
        assert_eq!(
            after[work as usize].1,
            if payload_installed {
                literal(1, &format!("sequence/work{work}-payload.after"))
            } else {
                before[work as usize].1.clone()
            }
        );
        if work == 2 {
            assert_eq!(after[1].1, before[1].1);
        }
        assert_eq!(
            after[0].0.counter,
            before[0].0.counter + u64::from(metadata_installed)
        );
        assert_eq!(
            after[work as usize].0.counter,
            u64::from(metadata_installed)
        );
        assert_eq!(after[work as usize].0.dirty, metadata_installed);
        assert_eq!(after[work as usize].0.entry.is_some(), registry_installed);
        assert_eq!(after[work as usize].2.is_some(), published);
        assert_eq!(
            writer.inner.base.descriptor.as_slice(),
            if metadata_installed {
                literal(1, &format!("sequence/work{work}-terminal.descriptor"))
            } else {
                descriptor
            }
        );
        assert!(writer.inner.installation.is_some());
        assert!(writer.inner.base.batch.is_some() && writer.inner.base.terminal_pending);
        assert!(after[0].0.claim_present);
        assert!(!after[0].0.initializer_pins_held);
        assert_eq!(after[0].0.total, 0);
        assert_eq!(state.lock().unwrap().files[wal::WAL_NAME].bytes, wal_before);
        assert_eq!(file_inventory(&state), files);
        assert_eq!(io_counts(&state), counts);
        let append = storage.wal().append.lock().unwrap();
        assert_eq!(
            (append.prefix.complete_lsn, append.prefix.synced_lsn),
            (2 * work, 2 * work - 1)
        );
        let expected = literal(1, &format!("sequence/work{work}-terminal.wal"));
        assert_eq!(&append.slot.as_ref().unwrap()[..expected.len()], expected);
    });
}
scenario!(
    mixed_owner_status_init_stage_owned_unwind,
    "MO-INIT-S-01",
    init_partial(SystemPageKind::Status, 0)
);
scenario!(
    mixed_owner_status_init_allocator_image_unwind,
    "MO-INIT-S-02",
    init_partial(SystemPageKind::Status, 1)
);
scenario!(
    mixed_owner_status_init_payload_image_unwind,
    "MO-INIT-S-03",
    init_partial(SystemPageKind::Status, 2)
);
scenario!(
    mixed_owner_status_init_metadata_unwind,
    "MO-INIT-S-04",
    init_partial(SystemPageKind::Status, 3)
);
scenario!(
    mixed_owner_status_init_registry_unwind,
    "MO-INIT-S-05",
    init_partial(SystemPageKind::Status, 4)
);
scenario!(
    mixed_owner_status_init_publication_unwind,
    "MO-INIT-S-06",
    init_partial(SystemPageKind::Status, 5)
);
scenario!(
    mixed_owner_status_init_dpt_guard_unwind,
    "MO-INIT-S-07",
    init_partial(SystemPageKind::Status, 6)
);
scenario!(
    mixed_owner_status_init_dpt_precheck_unwind,
    "MO-INIT-S-08",
    init_partial(SystemPageKind::Status, 7)
);
scenario!(
    mixed_owner_undo_init_stage_owned_unwind,
    "MO-INIT-U-01",
    init_partial(SystemPageKind::UndoRoot, 0)
);
scenario!(
    mixed_owner_undo_init_allocator_image_unwind,
    "MO-INIT-U-02",
    init_partial(SystemPageKind::UndoRoot, 1)
);
scenario!(
    mixed_owner_undo_init_payload_image_unwind,
    "MO-INIT-U-03",
    init_partial(SystemPageKind::UndoRoot, 2)
);
scenario!(
    mixed_owner_undo_init_metadata_unwind,
    "MO-INIT-U-04",
    init_partial(SystemPageKind::UndoRoot, 3)
);
scenario!(
    mixed_owner_undo_init_registry_unwind,
    "MO-INIT-U-05",
    init_partial(SystemPageKind::UndoRoot, 4)
);
scenario!(
    mixed_owner_undo_init_publication_unwind,
    "MO-INIT-U-06",
    init_partial(SystemPageKind::UndoRoot, 5)
);
scenario!(
    mixed_owner_undo_init_dpt_guard_unwind,
    "MO-INIT-U-07",
    init_partial(SystemPageKind::UndoRoot, 6)
);
scenario!(
    mixed_owner_undo_init_dpt_precheck_unwind,
    "MO-INIT-U-08",
    init_partial(SystemPageKind::UndoRoot, 7)
);

fn init_wal_failure(kind: SystemPageKind, partial_write: bool) {
    fixture!(1, |bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let work = prepare_class!(bound, owner, kind);
        let (mut writer, _, _) = bound.split();
        let reservation = writer.reserve(&mut owner, kind).unwrap();
        let target = reservation.target();
        let start = state.lock().unwrap().events.len();
        let home_before = state.lock().unwrap().files[HOME].bytes.clone();
        state.lock().unwrap().plan = Some(port::Plan {
            at: start + usize::from(!partial_write),
            action: if partial_write {
                port::Action::PrefixError
            } else {
                port::Action::BeforeError
            },
        });
        match reservation.initialize() {
            Err(InitAttemptError::Fatal(error)) => {
                assert_eq!(error.effect, Effect::CallerMustResolve)
            }
            _ => panic!("observable INIT WAL failure must not return an unchanged reservation"),
        }
        assert!(storage.session_fence().is_fenced());
        let current = snapshot(writer.inner.pool);
        assert_eq!(
            current[0].1,
            literal(1, &format!("sequence/work{work}-allocator.after"))
        );
        assert_eq!(
            current[work as usize].1,
            literal(1, &format!("sequence/work{work}-payload.after"))
        );
        assert_eq!(
            current[work as usize].2.unwrap().0.identity.page,
            target.page
        );
        assert!(writer.inner.installation.is_some());
        assert!(writer.inner.base.batch.is_some() && writer.inner.base.terminal_pending);
        assert!(current[0].0.claim_present && !current[0].0.initializer_pins_held);
        assert_eq!(current[0].0.total, 0);
        let mut expected = Vec::new();
        for prior in 1..work {
            expected.extend(literal(1, &format!("sequence/work{prior}-reserve.wal")));
            expected.extend(literal(1, &format!("sequence/work{prior}-terminal.wal")));
        }
        expected.extend(literal(1, &format!("sequence/work{work}-reserve.wal")));
        let terminal = literal(1, &format!("sequence/work{work}-terminal.wal"));
        let count = if partial_write {
            terminal.len() / 2
        } else {
            terminal.len()
        };
        expected.extend_from_slice(&terminal[..count]);
        let state = state.lock().unwrap();
        assert_eq!(&state.files[wal::WAL_NAME].bytes[4096..], expected);
        assert_eq!(state.files[HOME].bytes, home_before);
        assert_eq!(state.hit, 1);
    });
}
scenario!(
    mixed_owner_status_init_partial_wal_keeps_published_obligation,
    "MO-INIT-WAL-S-PARTIAL",
    init_wal_failure(SystemPageKind::Status, true)
);
scenario!(
    mixed_owner_undo_init_partial_wal_keeps_published_obligation,
    "MO-INIT-WAL-U-PARTIAL",
    init_wal_failure(SystemPageKind::UndoRoot, true)
);
scenario!(
    mixed_owner_status_init_sync_error_keeps_published_obligation,
    "MO-INIT-WAL-S-SYNC",
    init_wal_failure(SystemPageKind::Status, false)
);
scenario!(
    mixed_owner_undo_init_sync_error_keeps_published_obligation,
    "MO-INIT-WAL-U-SYNC",
    init_wal_failure(SystemPageKind::UndoRoot, false)
);

fn retired_continuation(kind: SystemPageKind, fence_retained: bool) {
    fixture!(1, |bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let (a, b) = two_dirty!(bound, owner);
        let (target, work) = if kind == SystemPageKind::Status {
            (a.target, 1)
        } else {
            (b.target, 2)
        };
        let before = snapshot(&bound.inner.inner.pool);
        let start = state.lock().unwrap().events.len();
        let (arrived, arrival) = mpsc::sync_channel(1);
        let (resume, release) = mpsc::sync_channel(1);
        state.lock().unwrap().pause = Some(port::Pause {
            at: start + 30,
            after: true,
            entered: arrived,
            release,
            unwind: false,
        });
        {
            let (_, reader, mut flusher) = bound.split();
            let mut reader_owner = core.try_owner(OwnerRole::Foreground).unwrap();
            let guard = std::thread::scope(|scope| {
                let task = scope.spawn(|| {
                    let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                    flusher.try_flush(&mut dedicated, target).unwrap()
                });
                arrival.recv_timeout(Duration::from_secs(5)).unwrap();
                let guard = reader.read(&mut reader_owner, target).unwrap();
                resume.send(()).unwrap();
                assert_eq!(task.join().unwrap(), FlushProgress::RetiredPendingFinalize);
                guard
            });
            let retained = flusher.snapshot();
            assert!(retained.io.homes_synced && retained.io.retirement_selected);
            assert_eq!((retained.io.attempted, retained.io.completed), (31, 31));
            assert!(flusher.inner.control.retired.is_some());
            assert!(flusher.inner.control.capture.is_some());
            let files = file_inventory(&state);
            let counts = io_counts(&state);
            let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
            if fence_retained {
                storage.session_fence().fence();
                assert_eq!(
                    flusher.continue_flush(&mut dedicated).unwrap_err().effect,
                    Effect::CallerMustResolve
                );
                let _ = guard.release_checked();
            } else {
                assert_eq!(
                    flusher
                        .try_flush_allocator(&mut dedicated)
                        .unwrap_err()
                        .effect,
                    Effect::Unchanged
                );
                assert_eq!(
                    flusher.continue_flush(&mut dedicated).unwrap(),
                    FlushProgress::RetiredPendingFinalize
                );
                guard.release_checked().unwrap();
            }
            assert_eq!(flusher.snapshot().phase, retained.phase);
            assert_eq!(flusher.snapshot().io, retained.io);
            assert_eq!(file_inventory(&state), files);
            assert_eq!(io_counts(&state), counts);
        }
        let counts = io_counts(&state);
        let files = file_inventory(&state);
        if fence_retained {
            assert!(bound.inner.flush.retired.is_some());
            assert!(core.try_owner(OwnerRole::DedicatedFlusher).is_err());
        } else {
            let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
            let mut flusher = bound.split().2;
            let completed = match flusher.continue_flush(&mut dedicated).unwrap() {
                FlushProgress::Complete(value) => value,
                other => panic!("retained continuation must finalize {other:?}"),
            };
            assert!(completed.finalized.cleaned);
            assert!(flusher.inner.control.retired.is_none());
            assert!(flusher.inner.control.capture.is_none());
        }
        assert_eq!(io_counts(&state), counts);
        assert_eq!(file_inventory(&state), files);
        let current = snapshot(&bound.inner.inner.pool);
        assert_eq!(current[work].1, before[work].1);
        assert_eq!(current[work].0.dirty, fence_retained);
        let state = state.lock().unwrap();
        let offset = target.page.page_no as usize * PAGE_BYTES;
        assert_eq!(
            &state.files[HOME].bytes[offset..offset + PAGE_BYTES],
            literal(1, &format!("sequence/work{work}-payload.after"))
        );
        assert_eq!(state.events.len(), start + 31);
    });
}
scenario!(
    mixed_owner_status_retired_view_reentry_does_not_rewrite_home,
    "MO-RETAINED-S",
    retired_continuation(SystemPageKind::Status, false)
);
scenario!(
    mixed_owner_undo_retired_view_reentry_does_not_rewrite_home,
    "MO-RETAINED-U",
    retired_continuation(SystemPageKind::UndoRoot, false)
);
scenario!(
    mixed_owner_undo_retired_fence_preserves_obligation,
    "MO-RETAINED-U-FENCED",
    retired_continuation(SystemPageKind::UndoRoot, true)
);

fn waiting_wal_continuation(kind: SystemPageKind) {
    fixture!(1, |bound, core, state, storage| {
        let mut setup_owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let work = prepare_class!(bound, setup_owner, kind);
        drop(setup_owner);
        let target;
        {
            let (mut writer, _, mut flusher) = bound.split();
            let (ready, reservation_ready) = mpsc::sync_channel(1);
            let (start, proceed) = mpsc::sync_channel(1);
            let (arrived, arrival) = mpsc::sync_channel(1);
            let (resume, release) = mpsc::sync_channel(1);
            target = std::thread::scope(|scope| {
                let core_ref = &core;
                let task = scope.spawn(move || {
                    let mut owner = core_ref.try_owner(OwnerRole::Maintenance).unwrap();
                    let reservation = writer.reserve(&mut owner, kind).unwrap();
                    ready.send(reservation.target()).unwrap();
                    proceed.recv_timeout(Duration::from_secs(5)).unwrap();
                    initialized(reservation)
                });
                let target = reservation_ready
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap();
                let at = state.lock().unwrap().events.len() + 1;
                state.lock().unwrap().pause = Some(port::Pause {
                    at,
                    after: false,
                    entered: arrived,
                    release,
                    unwind: false,
                });
                start.send(()).unwrap();
                arrival.recv_timeout(Duration::from_secs(5)).unwrap();
                let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                assert_eq!(
                    flusher.try_flush(&mut dedicated, target).unwrap(),
                    FlushProgress::WaitingWal
                );
                let retained = flusher.snapshot();
                assert!(flusher.inner.control.capture.is_some());
                assert!(flusher.inner.control.authority.is_some());
                let counts = io_counts(&state);
                let files = file_inventory(&state);
                assert_eq!(
                    flusher
                        .try_flush_allocator(&mut dedicated)
                        .unwrap_err()
                        .effect,
                    Effect::Unchanged
                );
                assert_eq!(
                    flusher.continue_flush(&mut dedicated).unwrap(),
                    FlushProgress::WaitingWal
                );
                assert_eq!(flusher.snapshot().phase, retained.phase);
                assert_eq!(flusher.snapshot().io, retained.io);
                assert_eq!(io_counts(&state), counts);
                assert_eq!(file_inventory(&state), files);
                resume.send(()).unwrap();
                let observed = task.join().unwrap();
                assert_eq!(observed.target, target);
                assert_eq!(observed.lsn, PageLsn(2 * work));
                target
            });
        }
        let start = state.lock().unwrap().events.len();
        let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let completed = match bound.split().2.continue_flush(&mut dedicated).unwrap() {
            FlushProgress::Complete(value) => value,
            other => panic!("WAL completion must continue the original capture {other:?}"),
        };
        assert!(completed.finalized.cleaned);
        assert_eq!(completed.captured.identity.page, target.page);
        assert_eq!(state.lock().unwrap().events.len(), start + 31);
        assert_eq!(
            snapshot(&bound.inner.inner.pool)[work as usize].1,
            literal(1, &format!("sequence/work{work}-payload.after"))
        );
        assert!(!snapshot(&bound.inner.inner.pool)[work as usize].0.dirty);
        assert!(!storage.session_fence().is_fenced());
    });
}
scenario!(
    mixed_owner_status_waiting_wal_keeps_capture_through_view_drop,
    "MO-WAITING-WAL-S",
    waiting_wal_continuation(SystemPageKind::Status)
);
scenario!(
    mixed_owner_undo_waiting_wal_keeps_capture_through_view_drop,
    "MO-WAITING-WAL-U",
    waiting_wal_continuation(SystemPageKind::UndoRoot)
);

#[path = "tests/load_addendum.rs"]
mod load_addendum;
