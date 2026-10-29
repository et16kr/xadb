//! New closed-INIT ownership, atomic admission, cut, and concurrency cases.
//! All expected images come from the independent Status INIT literal inventory.
use super::*;
use crate::storage::allocation_page::SystemWorkKind;
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::mpsc::{Receiver, sync_channel},
    time::Duration,
};
const STEP: Duration = Duration::from_secs(5);
fn ack(rx: &Receiver<()>) {
    rx.recv_timeout(STEP).expect("bounded actual protocol cut");
}
fn frame_failure(error: StatusInitError, cause: dirty_frame::Cause, effect: Effect) {
    assert_eq!(error.effect, effect);
    match error.cause {
        Cause::Frame(actual) => {
            assert_eq!(actual.cause, cause);
            assert_eq!(actual.effect, effect);
        }
        other => panic!("wrong origin {other:?}"),
    }
}
fn raw_prefix(storage: &MutableStorageOwner<'_, '_, port::Writer>) -> WalPrefix {
    storage
        .wal()
        .append
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .prefix
}

fn completed(id: &str) {
    if !id.starts_with("CF") {
        println!("XADB_STATUS_INIT_S5_CASE {id}");
    } else {
        mark(id);
    }
}
fn authority_case(profile: u32, variant: usize, id: &str) {
    storage_fixture!(profile, |storage, state| {
        let foreign_fence = SessionFence::new();
        let foreign_binding = AtomicU8::new(BOUND);
        let foreign_core = PageConcurrency::new(storage.session_fence()).unwrap();
        let core = PageConcurrency::new(storage.session_fence()).unwrap();
        let mut allocator = boxed::<PAGE_BYTES>(0x2d);
        let mut payload = boxed::<PAGE_BYTES>(0xa5);
        let mut operation = boxed::<OPERATION_BYTES>(0x73);
        let mut frames = [
            DirtyFrame::new(&mut allocator),
            DirtyFrame::new(&mut payload),
        ];
        let mut bound = ok(storage.bind_status_bootstrap(&core, &mut frames, &mut operation));
        let (mut writer, _) = bound.split();
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let first = writer.reserve_one(&mut owner).unwrap();
        let old = (first.batch, first.page, first.ordinal);
        let mut reservation = if variant == 9 {
            cancel(first);
            writer.reserve_one(&mut owner).unwrap()
        } else {
            first
        };
        let current = (reservation.batch, reservation.page, reservation.ordinal);
        reset(&state);
        let before_a = diagnose(reservation.writer.pool, 0);
        let before_s = diagnose(reservation.writer.pool, 1);
        let before_wal = wal_bytes(&state);
        if variant <= 3 {
            let mut authority = reservation
                .writer
                .authority(reservation.batch, reservation.page, 0, reservation.kind)
                .unwrap();
            match variant {
                1 => authority.binding = &foreign_binding,
                2 => authority.core = &foreign_core,
                3 => authority.fence = &foreign_fence,
                _ => unreachable!(),
            }
            let rejected = err(reservation
                .writer
                .pool
                .claim_init_targets(reservation.owner, &authority));
            assert_eq!(rejected.cause, dirty_frame::Cause::BindingMismatch);
            assert_eq!(rejected.effect, Effect::Unchanged);
        } else {
            match variant {
                4 => {
                    reservation.batch.owner = SystemWorkRef::new(
                        current.0.owner.namespace(),
                        current.0.owner.kind(),
                        current.0.owner.id() + 1,
                        current.0.owner.generation(),
                    )
                    .unwrap()
                }
                5 => reservation.batch.birth += 1,
                6 => {
                    reservation.batch.owner = SystemWorkRef::new(
                        current.0.owner.namespace(),
                        SystemWorkKind::StructuralChange,
                        current.0.owner.id(),
                        current.0.owner.generation(),
                    )
                    .unwrap()
                }
                7 => {
                    reservation.page.incarnation =
                        Generation::new(current.1.incarnation.get() + 1).unwrap()
                }
                8 => reservation.ordinal = 1,
                9 => {
                    reservation.batch = old.0;
                    reservation.page = old.1;
                    reservation.ordinal = old.2;
                }
                _ => unreachable!(),
            }
            let attempted = (reservation.batch, reservation.page, reservation.ordinal);
            let writer_origin = std::ptr::from_ref(&*reservation.writer);
            let owner_origin = std::ptr::from_ref(&*reservation.owner);
            reservation = match reservation.initialize() {
                Err(InitAttemptError::Before(before)) => {
                    assert_eq!(before.error.effect, Effect::Unchanged);
                    assert_eq!(before.error.phase, Phase::Prepare);
                    assert!(matches!(before.error.cause, Cause::StaleReservation));
                    assert_eq!(
                        (
                            before.reservation.batch,
                            before.reservation.page,
                            before.reservation.ordinal
                        ),
                        attempted
                    );
                    assert_eq!(
                        std::ptr::from_ref(&*before.reservation.writer),
                        writer_origin
                    );
                    assert_eq!(std::ptr::from_ref(&*before.reservation.owner), owner_origin);
                    before.reservation
                }
                _ => panic!("same reservation must return before stage"),
            };
        }
        assert_eq!(diagnose(reservation.writer.pool, 0), before_a);
        assert_eq!(diagnose(reservation.writer.pool, 1), before_s);
        assert_eq!(wal_bytes(&state), before_wal);
        assert!(state.lock().unwrap().events.is_empty());
        assert!(!storage.session_fence().is_fenced());
        reservation.batch = current.0;
        reservation.page = current.1;
        reservation.ordinal = current.2;
        cancel(reservation);
        completed(id);
    });
}
#[test]
fn status_init_cf03_exact_authority_and_returned_reservation() {
    for variant in 1..=9 {
        let id = format!("CF03-{variant:02}");
        if selected(&id) {
            authority_case(1, variant, &id);
        }
    }
}

#[test]
fn status_init_cf04_whole_two_pin_capacity() {
    if selected("CF04-01") {
        fixture!(1, |bound, core, state| {
            let (mut writer, _) = bound.split();
            let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
            let reserved = writer.reserve_one(&mut owner).unwrap();
            let pool = reserved.writer.pool;
            let authority = reserved
                .writer
                .authority(reserved.batch, reserved.page, 0, reserved.kind)
                .unwrap();
            // The first target can be full while the closed payload has no pins.
            frame_test::seed_pins(pool, 0, 34, 34);
            let before = frame_init::pin_state(pool);
            let rejected = err(pool.claim_init_targets(reserved.owner, &authority));
            assert_eq!(rejected.cause, dirty_frame::Cause::Capacity);
            assert_eq!(rejected.effect, Effect::Unchanged);
            assert_eq!(frame_init::pin_state(pool), before);
            frame_test::seed_pins(pool, 0, 0, 0);
            cancel(reserved);
            assert!(!state.lock().unwrap().files.is_empty());
            mark("CF04-01");
        });
    }
    if selected("CF04-02") {
        fixture!(1, |bound, core, state| {
            let (mut writer, reader) = bound.split();
            let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
            init(writer.reserve_one(&mut owner).unwrap());
            let pool = writer.pool;
            assert!(pool.initialized_request().unwrap().is_some());
            // Capacity injection is a healthy published census boundary. The
            // actual maximum of 34 callers/68 real pins is observed in CF04-03.
            frame_test::seed_pins(pool, 1, 34, 34);
            let before = frame_init::pin_state(pool);
            let rejected = err(reader.read(&mut owner));
            frame_failure(rejected, dirty_frame::Cause::Capacity, Effect::Unchanged);
            assert_eq!(frame_init::pin_state(pool), before);
            assert_eq!(before.0[0], 0); // No partial first-target increment.
            frame_test::seed_pins(pool, 1, 0, 0);
            assert_eq!(frame_init::pin_state(pool).1, 0);
            assert!(state.lock().unwrap().files.contains_key(wal::WAL_NAME));
            mark("CF04-02");
        });
    }
    if selected("CF04-03") {
        fixture!(1, |bound, core, state| {
            let (mut writer, reader) = bound.split();
            let mut initializer = core.try_owner(OwnerRole::Maintenance).unwrap();
            init(writer.reserve_one(&mut initializer).unwrap());
            let mut owners: Vec<_> = (0..32)
                .map(|_| core.try_owner(OwnerRole::Foreground).unwrap())
                .collect();
            owners.push(core.try_owner(OwnerRole::DedicatedFlusher).unwrap());
            let mut reads = Vec::new();
            for owner in &mut owners {
                reads.push(reader.read(owner).unwrap());
            }
            let last = reader.read(&mut initializer).unwrap();
            let pins = frame_init::pin_state(writer.pool);
            assert_eq!((pins.0[0], pins.0[1], pins.1), (34, 34, 68));
            assert!(!pins.2 && !pins.3);
            last.release_checked().unwrap();
            for read in reads {
                read.release_checked().unwrap();
            }
            assert_eq!(frame_init::pin_state(writer.pool).1, 0);
            assert!(!writer.storage.session_fence().is_fenced());
            assert!(state.lock().unwrap().files.contains_key(wal::WAL_NAME));
            mark("CF04-03");
        });
    }
}

#[test]
fn status_init_cf05_actual_content_busy_returns_whole_claim() {
    for index in 0..2 {
        if !selected(&format!("CF05-{:02}", index + 1)) {
            continue;
        }
        fixture!(1, |bound, core, state| {
            let (mut writer, _) = bound.split();
            let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
            let reservation = writer.reserve_one(&mut owner).unwrap();
            let pool = reservation.writer.pool;
            let before_a = diagnose(pool, 0);
            let before_s = diagnose(pool, 1);
            let before_wal = wal_bytes(&state);
            reset(&state);
            let (arrived, arrival) = sync_channel(1);
            let (release, resume) = sync_channel(1);
            let reservation = std::thread::scope(|scope| {
                let holder =
                    scope.spawn(move || frame_init::hold_content(pool, index, arrived, resume));
                ack(&arrival);
                let reservation = match reservation.initialize() {
                    Err(InitAttemptError::Before(before)) => {
                        assert_eq!(before.error.phase, Phase::Revalidate);
                        frame_failure(before.error, dirty_frame::Cause::Busy, Effect::Unchanged);
                        before.reservation
                    }
                    _ => panic!("actual content Busy must return reservation"),
                };
                assert_eq!(frame_init::pin_state(pool).1, 0);
                release.try_send(()).unwrap();
                holder.join().unwrap();
                reservation
            });
            assert_eq!(diagnose(pool, 0), before_a);
            assert_eq!(diagnose(pool, 1), before_s);
            assert_eq!(wal_bytes(&state), before_wal);
            assert!(state.lock().unwrap().events.is_empty());
            cancel(reservation);
            mark(&format!("CF05-{:02}", index + 1));
        });
    }
}

#[test]
fn status_init_cf06_stale_and_closed_corruption() {
    for variant in 1..=8 {
        if !selected(&format!("CF06-{variant:02}")) {
            continue;
        }
        if variant == 7 {
            // Original registration is immutable after binding. Exercise this
            // corruption before bind, rather than introducing a live raw setter.
            storage_fixture!(1, |storage, state| {
                let core = PageConcurrency::new(storage.session_fence()).unwrap();
                let mut allocator = boxed::<PAGE_BYTES>(0x2d);
                let mut payload = boxed::<PAGE_BYTES>(0xa5);
                let mut operation = boxed::<OPERATION_BYTES>(0x73);
                let mut frames = [
                    DirtyFrame::new(&mut allocator),
                    DirtyFrame::new(&mut payload),
                ];
                let identity = wal::bootstrap::fixed_entry()
                    .unwrap()
                    .metadata_identity(storage.profile().spec().namespace, 0)
                    .unwrap();
                frame_init::seed_original_registration(
                    &mut frames[1],
                    allocator_request(identity),
                    1,
                );
                let rejected =
                    err(storage.bind_status_bootstrap(&core, &mut frames, &mut operation));
                assert_eq!(rejected.effect, Effect::CallerMustResolve);
                assert!(matches!(rejected.cause, StorageCause::Frame(_)));
                assert_eq!(storage.binding.load(Ordering::Acquire), AVAILABLE);
                assert!(state.lock().unwrap().events.is_empty());
                assert!(state.lock().unwrap().read_events.is_empty());
                assert!(payload.iter().all(|byte| *byte == 0xa5));
                mark("CF06-07");
            });
            continue;
        }
        fixture!(1, |bound, core, state| {
            let (mut writer, _) = bound.split();
            let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
            let reserved = writer.reserve_one(&mut owner).unwrap();
            let pool = reserved.writer.pool;
            let before_a = diagnose(pool, 0);
            let before_s = diagnose(pool, 1);
            let before_wal = wal_bytes(&state);
            reset(&state);
            if variant <= 2 {
                let authority = reserved
                    .writer
                    .authority(reserved.batch, reserved.page, 0, reserved.kind)
                    .unwrap();
                let write = pool
                    .claim_init_targets(reserved.owner, &authority)
                    .unwrap()
                    .try_write()
                    .unwrap();
                let mut snapshot = boxed::<PAGE_BYTES>(0);
                let mut stamp = write.copy_allocator_snapshot(&mut snapshot).unwrap();
                if variant == 1 {
                    stamp.modification_counter += 1;
                } else {
                    let mut stale = before_a.1.entry.unwrap();
                    stale.modification_counter += 1;
                    frame_test::seed_dpt(pool, 0, Some(stale));
                }
                let failure = err(write.preflight(&stamp));
                assert_eq!(
                    failure.cause,
                    if variant == 1 {
                        dirty_frame::Cause::Stale
                    } else {
                        dirty_frame::Cause::Invariant
                    }
                );
                assert_eq!(
                    failure.effect,
                    if variant == 1 {
                        Effect::Unchanged
                    } else {
                        Effect::CallerMustResolve
                    }
                );
                let cleanup = write.release_checked();
                if variant == 1 {
                    cleanup.unwrap();
                } // The first fatal remains observed above.
                assert_eq!(&diagnose(pool, 0).0[..], &before_a.0[..]);
                assert_eq!(diagnose(pool, 1), before_s);
                assert_eq!(wal_bytes(&state), before_wal);
                assert!(state.lock().unwrap().events.is_empty());
                if variant == 1 {
                    // Stale preflight was unchanged. The returned healthy
                    // reservation can still perform its separate real cancel.
                    let cancelled = cancel(reserved);
                    assert_eq!(cancelled.page_lsn, PageLsn(2));
                    assert_eq!(cancelled.wal.complete_lsn, 2);
                    assert_eq!(cancelled.wal.synced_lsn, 2);
                    assert_history(&writer, &state, 1, 0, "cancel");
                    assert_eq!(diagnose(pool, 1), before_s);
                    let expected = literal(1, 0, "cancel", "work1-cancel.wal");
                    let state = state.lock().unwrap();
                    assert_eq!(state.events.len(), 2);
                    let write = &state.events[0];
                    assert_eq!(
                        (write.kind, write.name.as_str(), write.offset, write.length),
                        (
                            "write",
                            wal::WAL_NAME,
                            before_wal.len() as u64,
                            expected.len()
                        )
                    );
                    assert_eq!(write.data, expected);
                    assert_eq!(write.returned, Some(expected.len()));
                    let sync = &state.events[1];
                    assert_eq!(
                        (sync.kind, sync.name.as_str(), sync.offset, sync.length),
                        ("sync", wal::WAL_NAME, 0, 0)
                    );
                    assert!(sync.data.is_empty());
                    assert_eq!(sync.returned, None);
                } else {
                    drop(reserved);
                }
            } else {
                match variant {
                    3 => frame_init::seed_closed_metadata(pool, PageLsn(1), 0, None, false),
                    4 => frame_init::seed_closed_metadata(pool, PageLsn(0), 1, None, false),
                    5 => frame_init::seed_closed_metadata(
                        pool,
                        PageLsn(0),
                        0,
                        Some(PageLsn(1)),
                        false,
                    ),
                    6 => frame_init::seed_closed_metadata(pool, PageLsn(0), 0, None, true),
                    8 => {
                        let authority = reserved
                            .writer
                            .authority(reserved.batch, reserved.page, 0, reserved.kind)
                            .unwrap();
                        // Generation now belongs to the exclusively prepared
                        // slot; a shared publisher cannot inject generation0.
                        // Inject a wrong-kind request into the claimed cell to
                        // retain this post-reservation corruption boundary.
                        let mut wrong = authority.payload;
                        wrong.kind = PageKind::RowLeaf;
                        assert!(frame_init::seed_publication(pool, wrong, 1));
                    }
                    _ => unreachable!(),
                }
                let corrupted = diagnose(pool, 1);
                // Publication is pool-wide diagnostic state. Preserve the
                // deliberately injected value on both snapshots, while proving
                // the allocator's page and per-frame metadata remain unchanged.
                let mut expected_allocator = before_a;
                if variant == 8 {
                    expected_allocator.1.published = corrupted.1.published;
                }
                let failure = match reserved.initialize() {
                    Err(InitAttemptError::Fatal(error)) => error,
                    _ => panic!("closed invariant must be fatal"),
                };
                assert_eq!(
                    failure.phase,
                    if variant == 8 {
                        Phase::Prepare
                    } else {
                        Phase::Revalidate
                    }
                );
                frame_failure(
                    failure,
                    dirty_frame::Cause::Invariant,
                    Effect::CallerMustResolve,
                );
                assert_eq!(diagnose(pool, 0), expected_allocator);
                assert_eq!(diagnose(pool, 1), corrupted);
                assert_eq!(&corrupted.0[..], &before_s.0[..]);
                assert_eq!(wal_bytes(&state), before_wal);
            }
            if variant != 1 {
                assert!(state.lock().unwrap().events.is_empty());
            }
            mark(&format!("CF06-{variant:02}"));
        });
    }
}

const CUTS: [frame_init::InitPoint; 8] = [
    frame_init::InitPoint::StageOwned,
    frame_init::InitPoint::AllocatorImageInstalled,
    frame_init::InitPoint::PayloadImageInstalled,
    frame_init::InitPoint::MetadataInstalled,
    frame_init::InitPoint::RegistryInstalled,
    frame_init::InitPoint::Published,
    frame_init::InitPoint::DptGuardOwnedBeforePrecheck,
    frame_init::InitPoint::DptPrecheckedBeforePrepared,
];
// Each cut's expected matrix is independently fixed here, not copied from the
// install implementation's own output or inferred from its phase field.
fn assert_partial(
    writer: &StatusBootstrapWriter<'_, '_, '_, '_, '_, '_, '_, '_, port::Writer>,
    state: &Arc<Mutex<port::State>>,
    cut: usize,
    before_a: &[u8],
    before_desc: &[u8],
    before_wal: &[u8],
) {
    let profile = writer.storage.profile().spec().id;
    let a = diagnose(writer.pool, 0);
    let s = diagnose(writer.pool, 1);
    let image_a = (2..=6).contains(&cut);
    let image_s = (3..=6).contains(&cut);
    let metadata = (4..=6).contains(&cut);
    let registry = (5..=6).contains(&cut);
    let published = cut == 6;
    let after_a = literal(profile, 0, "init", "work1-allocator.after");
    let after_s = literal(profile, 0, "init", "work1-status.after");
    assert_eq!(&a.0[..], if image_a { &after_a } else { before_a });
    if image_s {
        assert_eq!(&s.0[..], after_s);
    } else {
        assert!(s.0.iter().all(|byte| *byte == 0xa5));
    }
    assert_eq!(
        (a.1.page_lsn.0, s.1.page_lsn.0),
        (if image_a { 2 } else { 1 }, if image_s { 2 } else { 0 })
    );
    assert_eq!(
        (a.1.counter, s.1.counter),
        (if metadata { 2 } else { 1 }, if metadata { 1 } else { 0 })
    );
    assert_eq!(
        (a.1.rec_lsn, s.1.rec_lsn),
        (Some(PageLsn(1)), metadata.then_some(PageLsn(2)))
    );
    assert!(a.1.dirty);
    assert_eq!(s.1.dirty, metadata);
    assert_eq!(
        a.1.entry.unwrap().modification_counter,
        if registry { 2 } else { 1 }
    );
    assert_eq!(s.1.entry.is_some(), registry);
    if let Some(entry) = s.1.entry {
        assert_eq!(entry.rec_lsn, PageLsn(2));
        assert_eq!(entry.modification_counter, 1);
    }
    assert_eq!(a.1.requirement.unwrap().lsn, if registry { 2 } else { 1 });
    assert_eq!(s.1.requirement.map(|r| r.lsn), registry.then_some(2));
    assert_eq!(s.1.published.is_some(), published);
    assert!(a.1.claim_present && s.1.claim_present);
    assert!(!a.1.initializer_pins_held && !s.1.initializer_pins_held);
    assert_eq!((a.1.total, s.1.total), (0, 0));
    assert_eq!(
        &writer.base.descriptor[..],
        if metadata {
            literal(profile, 0, "init", "work1-init.descriptor")
        } else {
            before_desc.to_vec()
        }
    );
    assert!(writer.base.batch.is_some() && writer.base.terminal_pending);
    assert_eq!(
        writer.installation.as_ref().unwrap().phase,
        if published {
            InitInstallPhase::Published
        } else if registry {
            InitInstallPhase::RegistryInstalled
        } else if metadata {
            InitInstallPhase::DescriptorInstalled
        } else {
            InitInstallPhase::StageOwned
        }
    );
    let prefix = raw_prefix(writer.storage);
    assert_eq!((prefix.complete_lsn, prefix.synced_lsn), (2, 1));
    assert_eq!(
        (prefix.complete_offset, prefix.synced_offset),
        (before_wal.len() as u64 + 38000, before_wal.len() as u64)
    );
    assert_eq!(wal_bytes(state), before_wal);
    let append = writer.storage.wal().append.lock().unwrap();
    assert!(matches!(append.state, wal::SlotState::Complete(_)));
    let frame = literal(profile, 0, "init", "wal.frames-only");
    assert_eq!(&append.slot.as_ref().unwrap()[..38000], &frame[21200..]);
    assert!(state.lock().unwrap().events.is_empty());
}

fn cut_case(profile: u32, offset: usize, id: &str) {
    let point = CUTS[offset];
    fixture!(profile, |bound, core, state| {
        let (mut writer, _) = bound.split();
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let reserved = writer.reserve_one(&mut owner).unwrap();
        let pool = reserved.writer.pool;
        let fence = reserved.writer.storage.session_fence();
        let core_ref = &core;
        let before_a = diagnose(pool, 0).0;
        let before_desc = reserved.writer.base.descriptor.to_vec();
        let before_wal = wal_bytes(&state);
        let homes = fixed_files(&state);
        reset(&state);
        let (arrived, arrival) = sync_channel(1);
        let (resume, release) = sync_channel(1);
        let (fenced, fence_arrival) = sync_channel(1);
        let (unwind, unwind_release) = sync_channel(1);
        frame_init::install_init_pause(pool, point, arrived, release);
        frame_test::install_fence_pause(pool, fenced, unwind_release);
        std::thread::scope(|scope| {
            let controller = scope.spawn(move || {
                ack(&arrival);
                assert!(!fence.is_fenced());
                assert!(frame_test::content_busy(pool, 0) && frame_test::content_busy(pool, 1));
                assert!(core_ref.diagnostic_version_busy());
                if offset == 0 {
                    assert_eq!(frame_init::try_pin_state(pool).unwrap().1, 2);
                } else {
                    let held_leaf = frame_init::try_pin_state(pool).unwrap_err();
                    assert_eq!(held_leaf.cause, dirty_frame::Cause::Busy);
                    assert_eq!(held_leaf.effect, Effect::Unchanged);
                }
                assert_eq!(frame_init::dpt_busy(pool), offset != 0);
                resume.try_send(frame_test::Resume::Panic).unwrap();
                ack(&fence_arrival);
                assert!(fence.is_fenced());
                assert!(frame_test::content_busy(pool, 0) && frame_test::content_busy(pool, 1));
                assert!(core_ref.diagnostic_version_busy());
                if offset != 0 {
                    assert!(frame_init::dpt_busy(pool));
                }
                unwind.try_send(frame_test::Resume::Continue).unwrap();
            });
            assert!(
                catch_unwind(AssertUnwindSafe(|| {
                    let _ = reserved.initialize();
                }))
                .is_err()
            );
            controller.join().unwrap();
        });
        assert_partial(
            &writer,
            &state,
            offset + 1,
            &before_a[..],
            &before_desc,
            &before_wal,
        );
        assert_eq!(fixed_files(&state), homes);
        assert!(!core.diagnostic_version_busy());
        completed(id);
    });
}
#[test]
fn status_init_cf08_exact_partial_cuts_fence_before_release() {
    for offset in 0..CUTS.len() {
        let id = format!("CF08-{:02}", offset + 1);
        if selected(&id) {
            cut_case(1, offset, &id);
        }
    }
}

#[test]
fn status_init_cf09_poststage_poison_preserves_old_images() {
    if !selected("CF09-01") {
        return;
    }
    fixture!(1, |bound, core, state| {
        let (mut writer, _) = bound.split();
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let reserved = writer.reserve_one(&mut owner).unwrap();
        let pool = reserved.writer.pool;
        let before_a = diagnose(pool, 0).0;
        let before_desc = reserved.writer.base.descriptor.to_vec();
        let before_wal = wal_bytes(&state);
        let homes = fixed_files(&state);
        reset(&state);
        let (arrived, arrival) = sync_channel(1);
        let (resume, release) = sync_channel(1);
        frame_init::install_init_pause(pool, frame_init::InitPoint::StageOwned, arrived, release);
        let result = std::thread::scope(|scope| {
            let injector = scope.spawn(move || {
                ack(&arrival);
                frame_test::poison_dpt(pool);
                resume.try_send(frame_test::Resume::Continue).unwrap();
            });
            let failure = match reserved.initialize() {
                Err(InitAttemptError::Fatal(e)) => e,
                _ => panic!("poststage poison must retain fatal ownership"),
            };
            injector.join().unwrap();
            failure
        });
        assert_eq!(result.phase, Phase::Install);
        frame_failure(
            result,
            dirty_frame::Cause::Poisoned,
            Effect::CallerMustResolve,
        );
        assert_partial(&writer, &state, 1, &before_a[..], &before_desc, &before_wal);
        assert_eq!(fixed_files(&state), homes);
        assert!(writer.storage.session_fence().is_fenced());
        mark("CF09-01");
    });
}

#[test]
fn status_init_cf11_checked_cleanup_preserves_first_fatal() {
    for (case, variant) in [(1, 0), (2, 1), (2, 2), (3, 0), (4, 0)] {
        if !selected(&format!("CF11-{case:02}")) {
            continue;
        }
        fixture!(1, |bound, core, state| {
            let (mut writer, _) = bound.split();
            let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
            let reserved = writer.reserve_one(&mut owner).unwrap();
            let pool = reserved.writer.pool;
            let core_ref = &core;
            if case == 4 {
                frame_init::seed_closed_metadata(pool, PageLsn(0), 1, None, false);
            }
            let before_a = diagnose(pool, 0).0;
            let before_s = diagnose(pool, 1).0;
            let before_wal = wal_bytes(&state);
            reset(&state);
            let (held, held_ack) = sync_channel(1);
            let (holder_release, holder_resume) = sync_channel(1);
            let (cleanup, cleanup_ack) = sync_channel(1);
            let (resume, release) = sync_channel(1);
            frame_test::install_release_pause(pool, cleanup, release);
            let (failure, damaged) = std::thread::scope(|scope| {
                let holder = if case == 4 {
                    None
                } else {
                    Some(
                        scope.spawn(move || frame_init::hold_content(pool, 1, held, holder_resume)),
                    )
                };
                if case != 4 {
                    ack(&held_ack);
                }
                let injector = scope.spawn(move || {
                    ack(&cleanup_ack);
                    assert!(core_ref.diagnostic_version_busy());
                    match case {
                        1 | 4 => frame_test::poison_pins(pool),
                        2 => {
                            if variant == 1 {
                                frame_test::seed_pins(pool, 0, 2, 2)
                            } else {
                                frame_init::corrupt_init_claim(pool, 3)
                            }
                        }
                        3 => crate::transaction::concurrency::diagnostic_poison_state(core_ref),
                        _ => unreachable!(),
                    }
                    let damaged = frame_init::pin_state(pool);
                    resume.try_send(frame_test::Resume::Continue).unwrap();
                    damaged
                });
                let failure = match reserved.initialize() {
                    Err(InitAttemptError::Fatal(e)) => e,
                    _ => panic!("cleanup fatal must not return unchanged reservation"),
                };
                let damaged = injector.join().unwrap();
                if let Some(holder) = holder {
                    holder_release.try_send(()).unwrap();
                    holder.join().unwrap();
                }
                (failure, damaged)
            });
            let expected = if case == 2 || case == 4 {
                dirty_frame::Cause::Invariant
            } else {
                dirty_frame::Cause::Poisoned
            };
            assert_eq!(failure.phase, Phase::Revalidate);
            frame_failure(failure, expected, Effect::CallerMustResolve);
            if case == 2 {
                assert_eq!(frame_init::pin_state(pool), damaged);
            } else {
                assert_eq!(frame_init::pin_state(pool).1, 0);
            }
            assert_eq!(&diagnose(pool, 0).0[..], &before_a[..]);
            assert_eq!(&diagnose(pool, 1).0[..], &before_s[..]);
            assert_eq!(wal_bytes(&state), before_wal);
            assert!(state.lock().unwrap().events.is_empty());
            assert!(writer.storage.session_fence().is_fenced());
            if case == 2 {
                println!(
                    "XADB_STATUS_INIT_VARIANT CF11-02 {}",
                    if variant == 1 {
                        "count_mismatch"
                    } else {
                        "own_pins_flag"
                    }
                );
            }
            if case != 2 || variant == 2 {
                mark(&format!("CF11-{case:02}"));
            }
        });
    }
}

#[test]
fn status_init_cf12_publication_and_concurrent_reader() {
    if selected("CF12-01") {
        fixture!(1, |bound, core, state| {
            let (mut writer, reader) = bound.split();
            let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
            let reserved = writer.reserve_one(&mut owner).unwrap();
            let pool = reserved.writer.pool;
            let core_ref = &core;
            let (published, published_ack) = sync_channel(1);
            let (install_resume, install_release) = sync_channel(1);
            let (released, released_ack) = sync_channel(1);
            let (cleanup_resume, cleanup_release) = sync_channel(1);
            let (finished, finish_ack) = sync_channel(1);
            frame_init::install_init_pause(
                pool,
                frame_init::InitPoint::Published,
                published,
                install_release,
            );
            std::thread::scope(|scope| {
                let controller = scope.spawn(move || {
                    let mut reader_owner = core_ref.try_owner(OwnerRole::Foreground).unwrap();
                    ack(&published_ack);
                    let unpublished_observation = pool.initialized_request().unwrap_err();
                    assert_eq!(unpublished_observation.cause, dirty_frame::Cause::Busy);
                    assert_eq!(unpublished_observation.effect, Effect::Unchanged);
                    let held_leaf = frame_init::try_pin_state(pool).unwrap_err();
                    assert_eq!(held_leaf.cause, dirty_frame::Cause::Busy);
                    assert_eq!(held_leaf.effect, Effect::Unchanged);
                    let rejected = err(reader.read(&mut reader_owner));
                    assert_eq!(rejected.phase, Phase::Observe);
                    frame_failure(rejected, dirty_frame::Cause::Busy, Effect::Unchanged);
                    frame_test::install_release_pause(pool, released, cleanup_release);
                    assert_eq!(
                        frame_init::try_pin_state(pool).unwrap_err().cause,
                        dirty_frame::Cause::Busy
                    );
                    install_resume
                        .try_send(frame_test::Resume::Continue)
                        .unwrap();
                    ack(&released_ack);
                    assert!(pool.initialized_request().unwrap().is_some());
                    assert_eq!(frame_init::try_pin_state(pool).unwrap().1, 2);
                    let read = reader.read(&mut reader_owner).unwrap();
                    assert_eq!(
                        &read.bytes().unwrap()[..],
                        literal(1, 0, "init", "work1-status.after")
                    );
                    assert_eq!(frame_init::pin_state(pool).1, 4);
                    cleanup_resume
                        .try_send(frame_test::Resume::Continue)
                        .unwrap();
                    // Keep real ordinary pins through terminal claim completion.
                    ack(&finish_ack);
                    let pins = frame_init::pin_state(pool);
                    assert_eq!(
                        (pins.0[0], pins.0[1], pins.1, pins.2, pins.3),
                        (1, 1, 2, false, false)
                    );
                    read.release_checked().unwrap();
                });
                let observed = init(reserved);
                assert_eq!(observed.lsn, PageLsn(2));
                assert!(!frame_init::pin_state(pool).2);
                finished.try_send(()).unwrap();
                controller.join().unwrap();
            });
            assert!(writer.installation.is_none());
            assert_eq!(frame_init::pin_state(pool).1, 0);
            assert_eq!(
                &wal_bytes(&state)[4096..],
                literal(1, 0, "init", "wal.frames-only")
            );
            mark("CF12-01");
        });
    }
    if !selected("CF12-02") {
        return;
    }
    for (variant, point) in CUTS[..5].iter().copied().enumerate() {
        // Changed shared-leaf diagnostics can be selected without repeating C1.
        if std::env::var("XADB_STATUS_INIT_CF12_02_CUTS")
            .is_ok_and(|value| !value.split(',').any(|cut| cut == (variant + 1).to_string()))
        {
            continue;
        }
        fixture!(1, |bound, core, state| {
            let (mut writer, reader) = bound.split();
            let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
            let reserved = writer.reserve_one(&mut owner).unwrap();
            let pool = reserved.writer.pool;
            let core_ref = &core;
            let (arrived, arrival) = sync_channel(1);
            let (resume, release) = sync_channel(1);
            let (finished, finish_ack) = sync_channel(1);
            frame_init::install_init_pause(pool, point, arrived, release);
            std::thread::scope(|scope| {
                let controller = scope.spawn(move || {
                    let mut reader_owner = core_ref.try_owner(OwnerRole::Foreground).unwrap();
                    ack(&arrival);
                    let before = frame_init::try_pin_state(pool);
                    if variant == 0 {
                        assert_eq!(before.as_ref().unwrap().1, 2);
                    } else {
                        let held_leaf = before.as_ref().unwrap_err();
                        assert_eq!(held_leaf.cause, dirty_frame::Cause::Busy);
                        assert_eq!(held_leaf.effect, Effect::Unchanged);
                    }
                    assert_eq!(frame_init::dpt_busy(pool), variant != 0);
                    let rejected = err(reader.read(&mut reader_owner));
                    assert_eq!(rejected.effect, Effect::Unchanged);
                    assert!(matches!(rejected.cause, Cause::NotInitialized));
                    assert_eq!(frame_init::try_pin_state(pool), before);
                    resume.try_send(frame_test::Resume::Continue).unwrap();
                    ack(&finish_ack);
                    let read = reader.read(&mut reader_owner).unwrap();
                    assert_eq!(
                        &read.bytes().unwrap()[..],
                        literal(1, 0, "init", "work1-status.after")
                    );
                    read.release_checked().unwrap();
                });
                init(reserved);
                finished.try_send(()).unwrap();
                controller.join().unwrap();
            });
            assert_eq!(frame_init::pin_state(pool).1, 0);
            assert!(!writer.storage.session_fence().is_fenced());
            assert_eq!(
                &wal_bytes(&state)[4096..],
                literal(1, 0, "init", "wal.frames-only")
            );
            println!("XADB_STATUS_INIT_VARIANT CF12-02 C{}", variant + 1);
        });
    }
    mark("CF12-02");
}

#[allow(
    clippy::result_large_err,
    reason = "observe the real inline reservation-return API without changing its allocation or unwind behavior"
)]
fn sync_schedule(profile: u32, action: port::Action, panics: bool, id: &str) {
    fixture!(profile, |bound, core, state| {
        let (mut writer, _) = bound.split();
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let before_wal;
        let homes;
        {
            let reserved = writer.reserve_one(&mut owner).unwrap();
            before_wal = wal_bytes(&state);
            homes = fixed_files(&state);
            reset(&state);
            state.lock().unwrap().plan = Some(port::Plan { at: 1, action });
            let observed = catch_unwind(AssertUnwindSafe(|| reserved.initialize()));
            match observed {
                Err(_) => assert!(panics),
                Ok(Err(InitAttemptError::Fatal(error))) => {
                    assert!(!panics);
                    assert_eq!(error.phase, Phase::Sync);
                    assert_eq!(error.effect, Effect::CallerMustResolve);
                    let Cause::Wal(wal_error) = error.cause else {
                        panic!("original WAL cause lost")
                    };
                    assert_eq!(wal_error.phase, wal::Phase::Sync);
                    assert_eq!(wal_error.effect, Effect::CallerMustResolve);
                    let wal::Cause::Io(lower) = wal_error.cause else {
                        panic!("original lower sync cause lost")
                    };
                    assert_eq!(lower.code, foundation::error::ErrorCode::IoWriteUnknown);
                    assert_eq!(lower.context, "sync outcome unknown");
                    assert_eq!(lower.io_kind, Some(std::io::ErrorKind::Other));
                    assert_eq!(lower.effect(), Effect::CallerMustResolve);
                    assert_eq!(lower.certainty(), foundation::error::Certainty::Unknown);
                    assert_eq!(
                        lower.retry_scope(),
                        foundation::error::RetryScope::AfterRecovery
                    );
                }
                _ => panic!("actual failed sync must retain fatal obligation"),
            }
        }
        let a = diagnose(writer.pool, 0);
        let s = diagnose(writer.pool, 1);
        assert_eq!(
            &a.0[..],
            literal(profile, 0, "init", "work1-allocator.after")
        );
        assert_eq!(&s.0[..], literal(profile, 0, "init", "work1-status.after"));
        assert_eq!((a.1.page_lsn, s.1.page_lsn), (PageLsn(2), PageLsn(2)));
        assert_eq!((a.1.counter, s.1.counter), (2, 1));
        assert_eq!(
            (a.1.rec_lsn, s.1.rec_lsn),
            (Some(PageLsn(1)), Some(PageLsn(2)))
        );
        assert!(a.1.dirty && s.1.dirty);
        assert_eq!(a.1.requirement.unwrap().lsn, 2);
        assert_eq!(s.1.requirement.unwrap().lsn, 2);
        assert!(s.1.published.is_some() && s.1.claim_present && !s.1.initializer_pins_held);
        assert_eq!(s.1.total, 0);
        assert_eq!(
            writer.installation.as_ref().unwrap().phase,
            InitInstallPhase::Published
        );
        assert!(writer.base.batch.is_some() && writer.base.terminal_pending);
        assert!(writer.storage.session_fence().is_fenced());
        let prefix = raw_prefix(writer.storage);
        assert_eq!((prefix.complete_lsn, prefix.synced_lsn), (2, 1));
        assert_eq!(prefix.complete_offset, before_wal.len() as u64 + 38000);
        assert_eq!(
            &wal_bytes(&state)[4096..],
            literal(profile, 0, "init", "wal.frames-only")
        );
        assert_eq!(fixed_files(&state), homes);
        assert_eq!(state.lock().unwrap().hit, 1);
        completed(id);
    });
}
#[test]
#[ignore = "DEV3 final S5 expanded INIT authority inventory"]
fn status_init_s5_authority_expanded() {
    for profile in 2..=3 {
        for variant in 1..=9 {
            let id = format!("SA-P{profile}-{variant:02}");
            if selected(&id) {
                authority_case(profile, variant, &id);
            }
        }
    }
}
#[test]
#[ignore = "DEV3 final S5 expanded dirty-before install/sync schedules"]
fn status_init_s5_install_schedules_expanded() {
    for profile in 2..=3 {
        for offset in 0..6 {
            let id = format!("SS-P{profile}-C{}", offset + 1);
            if selected(&id) {
                cut_case(profile, offset, &id);
            }
        }
        for (name, action, panics) in [
            ("SYNC-BEFORE", port::Action::BeforeError, false),
            ("SYNC-AFTER-ERROR", port::Action::AfterError, false),
            ("SYNC-AFTER-PANIC", port::Action::AfterPanic, true),
        ] {
            let id = format!("SS-P{profile}-{name}");
            if selected(&id) {
                sync_schedule(profile, action, panics, &id);
            }
        }
    }
}
