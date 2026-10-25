//! Admission and exact retained authority for the new consumer only.
use super::*;
use std::panic::{AssertUnwindSafe, catch_unwind};

fn frame_error(error: StatusFlushError, expected: dirty_frame::Cause) {
    assert_eq!(error.effect, Effect::Unchanged);
    match error.cause {
        Cause::Frame(e) => assert_eq!((e.cause, e.effect), (expected, Effect::Unchanged)),
        other => panic!("{other:?}"),
    }
}
fn ordinary_owner(variant_name: &str, foreign: bool, role: OwnerRole) {
    storage_fixture!(1, |storage, state| {
        let other = PageConcurrency::new(storage.session_fence()).unwrap();
        let core = PageConcurrency::new(storage.session_fence()).unwrap();
        let mut a = boxed::<PAGE_BYTES>(0);
        let mut s = boxed::<PAGE_BYTES>(0);
        let mut work = boxed::<INIT_OPERATION_BYTES>(0);
        let mut frames = [DirtyFrame::new(&mut a), DirtyFrame::new(&mut s)];
        let mut bound = converted(
            ok(storage.bind_status_bootstrap(&core, &mut frames, &mut work)).into_with_flush(),
        );
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let mut writer_owner = ok(core.try_owner(OwnerRole::Maintenance));
        initialize(&mut writer, &mut writer_owner, 0);
        if !foreign && matches!(role, OwnerRole::Maintenance) {
            // There is one maintenance slot. Release the initializer before
            // testing the role rejection, so admission itself is reachable.
            drop(writer_owner);
        }
        let mut owner = ok(if foreign {
            other.try_owner(role)
        } else {
            core.try_owner(role)
        });
        if variant_name == "forgotten-Apply" {
            std::mem::forget(ok(owner.try_apply()));
        }
        reset(&state);
        let before = inventory(&state);
        let snapshot = flusher.snapshot();
        let expected = if foreign {
            dirty_frame::Cause::BindingMismatch
        } else {
            dirty_frame::Cause::Concurrency(crate::transaction::concurrency::Cause::InvalidRole)
        };
        frame_error(err(flusher.try_flush_status(&mut owner)), expected);
        assert_eq!(flusher.snapshot(), snapshot);
        assert_eq!(inventory(&state), before);
        no_io(&state);
        assert!(frame_test::flush_diagnose(flusher.pool).claim.is_none());
        assert!(!flusher.storage.session_fence().is_fenced());
    });
}
fn before_publication() {
    for (name, exhausted) in [("normal-headroom", false), ("exhausted-headroom", true)] {
        fixture!(1, |bound, core, state| {
            let (_, _, mut flusher) = bound.split_with_flush();
            if exhausted {
                io_test::seed_headroom(&mut flusher.control.io, Some(u64::MAX), None, None);
            }
            let before = flusher.snapshot();
            let registry = frame_test::flush_diagnose(flusher.pool);
            let mut owner = ok(core.try_owner(OwnerRole::DedicatedFlusher));
            let error = err(flusher.try_flush_status(&mut owner));
            assert!(matches!(error.cause, Cause::NotInitialized));
            assert_eq!(error.effect, Effect::Unchanged);
            assert_eq!(flusher.snapshot(), before);
            no_io(&state);
            let after = frame_test::flush_diagnose(flusher.pool);
            assert_eq!(after.last_attempt, registry.last_attempt);
            assert_eq!(after.claim, registry.claim);
        });
        variant("BA03", name);
    }
    fixture!(1, |bound, core, state| {
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let mut owner = ok(core.try_owner(OwnerRole::Maintenance));
        let reservation = ok(writer.reserve_one(&mut owner));
        let pool = reservation.writer.pool;
        let (arrived, ack) = mpsc::sync_channel(1);
        let (resume, release) = mpsc::sync_channel(1);
        init_test::install_init_pause(
            pool,
            init_test::InitPoint::RegistryInstalled,
            arrived,
            release,
        );
        reset(&state);
        std::thread::scope(|scope| {
            let flusher = &mut flusher;
            let core = &core;
            let state = &state;
            let worker = scope.spawn(move || {
                ack.recv_timeout(STEP).unwrap();
                assert!(init_test::dpt_busy(pool));
                let before = flusher.snapshot();
                let mut owner = ok(core.try_owner(OwnerRole::DedicatedFlusher));
                let failure = err(flusher.try_flush_status(&mut owner));
                assert!(matches!(failure.cause, Cause::NotInitialized));
                assert_eq!(failure.effect, Effect::Unchanged);
                assert_eq!(flusher.snapshot(), before);
                no_io(state);
                resume.send(frame_test::Resume::Continue).unwrap();
            });
            match reservation.initialize() {
                Ok(_) => {}
                _ => panic!("actual C5 initialization"),
            }
            worker.join().unwrap();
        });
        let registry = frame_test::flush_diagnose(flusher.pool);
        assert_eq!(registry.last_attempt, 0);
        assert!(registry.claim.is_none());
    });
    variant("BA03", "actual-C5-unpublished");
}
fn convert_boundaries() {
    for name in ["before-reserve", "after-cancel", "after-INIT"] {
        original_fixture!(1, |original, core, state, storage| {
            let mut original = original;
            {
                let (mut writer, _) = original.split();
                let mut owner = ok(core.try_owner(OwnerRole::Maintenance));
                if name == "after-cancel" {
                    match ok(writer.reserve_one(&mut owner)).cancel() {
                        Ok(_) => {}
                        _ => panic!("cancel"),
                    }
                } else if name == "after-INIT" {
                    initialize(&mut writer, &mut owner, 0);
                }
            }
            let before = diagnose(&original.pool);
            let descriptor = original.control.base.descriptor;
            let core_ptr = std::ptr::from_ref(original.pool.bound_core());
            reset(&state);
            let disk = inventory(&state);
            let converted = converted(original.into_with_flush());
            no_io(&state);
            assert_eq!(inventory(&state), disk);
            assert!(storage.workspace.lock().unwrap().is_none());
            assert_eq!(
                std::ptr::from_ref(converted.inner.pool.bound_core()),
                core_ptr
            );
            assert_eq!(converted.inner.control.base.descriptor, descriptor);
            let after = diagnose(&converted.inner.pool);
            assert_eq!((&after.0[..], &after.1[..]), (&before.0[..], &before.1[..]));
            assert_eq!(after.2.published, before.2.published);
            assert_eq!(after.2.counters, before.2.counters);
            assert_eq!(after.2.rec_lsns, before.2.rec_lsns);
            assert_eq!(after.2.requirements, before.2.requirements);
            assert!(!storage.session_fence().is_fenced());
            drop(converted);
        });
        variant("BA04", name);
    }
}
fn bad_workspace() {
    original_fixture!(1, |original, _core, state, storage| {
        let workspace = storage.workspace.lock().unwrap().take();
        assert!(workspace.is_some());
        let failure = match original.into_with_flush() {
            Err(IntoFlushError::Fatal(e)) => e,
            _ => panic!("missing workspace must fence"),
        };
        assert!(matches!(failure.cause, Cause::Invariant));
        assert_eq!(failure.effect, Effect::CallerMustResolve);
        assert!(storage.workspace.lock().unwrap().is_none());
        no_io(&state);
    });
    variant("BA05", "missing-workspace");
    original_fixture!(1, |original, _core, state, storage| {
        let (arrived, ack) = mpsc::sync_channel(1);
        let (resume, release) = mpsc::sync_channel(1);
        *ADAPTER_PAUSE.lock().unwrap() = Some(AdapterPause {
            binding: std::ptr::from_ref(&storage.binding) as usize,
            arrived,
            release,
        });
        let failure = std::thread::scope(|scope| {
            let storage = &storage;
            let poisoner = scope.spawn(move || {
                ack.recv_timeout(STEP).unwrap();
                assert!(
                    catch_unwind(AssertUnwindSafe(|| {
                        let _guard = storage.workspace.lock().unwrap();
                        panic!("poison workspace before take");
                    }))
                    .is_err()
                );
                resume.send(()).unwrap();
            });
            let failure = match original.into_with_flush() {
                Err(IntoFlushError::Fatal(e)) => e,
                _ => panic!("poison must fence"),
            };
            poisoner.join().unwrap();
            failure
        });
        assert_eq!(failure.effect, Effect::CallerMustResolve);
        match failure.cause {
            Cause::Storage(e) => {
                assert_eq!(e.effect, Effect::CallerMustResolve);
                assert!(matches!(
                    e.cause,
                    super::super::super::super::Cause::Poisoned
                ));
            }
            other => panic!("{other:?}"),
        }
        assert!(storage.workspace.lock().unwrap_err().into_inner().is_some());
        no_io(&state);
    });
    variant("BA05", "poison-before-take");
}
fn both_clean() {
    fixture!(1, |bound, core, state| {
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let mut owner = ok(core.try_owner(OwnerRole::Maintenance));
        initialize(&mut writer, &mut owner, 0);
        let mut dedicated = ok(core.try_owner(OwnerRole::DedicatedFlusher));
        done(ok(flusher.try_flush_allocator(&mut dedicated)));
        done(ok(flusher.try_flush_status(&mut dedicated)));
        for target in [false, true] {
            reset(&state);
            let snapshot = flusher.snapshot();
            let before = diagnose(flusher.pool);
            assert_eq!(
                ok(if target {
                    flusher.try_flush_status(&mut dedicated)
                } else {
                    flusher.try_flush_allocator(&mut dedicated)
                }),
                FlushProgress::NoDirty
            );
            assert_eq!(flusher.snapshot(), snapshot);
            no_io(&state);
            let after = diagnose(flusher.pool);
            assert_eq!(after.2.page_lsns, before.2.page_lsns);
            assert_eq!(after.2.counters, before.2.counters);
            assert!(after.2.published.is_some());
            assert!(after.2.dirty.iter().all(|v| !*v));
            variant("BA07", if target { "status" } else { "allocator" });
        }
        let failure = err(flusher.continue_flush(&mut dedicated));
        assert!(matches!(failure.cause, Cause::NoPending));
        assert_eq!(failure.effect, Effect::Unchanged);
        no_io(&state);
    });
}
fn altered_policy() {
    for (index, name) in ["binding", "core", "role"].into_iter().enumerate() {
        storage_fixture!(1, |storage, state| {
            let foreign_binding = AtomicU8::new(BOUND);
            let foreign = PageConcurrency::new(storage.session_fence()).unwrap();
            let core = PageConcurrency::new(storage.session_fence()).unwrap();
            let mut a = boxed::<PAGE_BYTES>(0);
            let mut s = boxed::<PAGE_BYTES>(0);
            let mut work = boxed::<{ super::super::super::OPERATION_BYTES }>(0);
            let mut frames = [DirtyFrame::new(&mut a), DirtyFrame::new(&mut s)];
            let mut bound = converted(
                ok(storage.bind_status_bootstrap(&core, &mut frames, &mut work)).into_with_flush(),
            );
            let (mut writer, _, flusher) = bound.split_with_flush();
            let mut w = ok(core.try_owner(OwnerRole::Maintenance));
            initialize(&mut writer, &mut w, 0);
            let mut o = ok(core.try_owner(OwnerRole::DedicatedFlusher));
            let mut authority = ok(flusher.status_authority());
            let request = *authority.request();
            let capture =
                ok(flusher
                    .pool
                    .capture_flush(&mut o, request, ok(flusher.control.io.image_mut())))
                .unwrap();
            match index {
                0 => authority.binding = &foreign_binding,
                1 => authority.core = &foreign,
                2 => {
                    authority.request.identity.owner =
                        foundation::identity::ObjectId::new(3).unwrap()
                }
                _ => unreachable!(),
            }
            reset(&state);
            let before = inventory(&state);
            let failure = err(flusher
                .control
                .io
                .prepare_status(&storage, &capture, &authority));
            assert_eq!(failure.effect, Effect::CallerMustResolve);
            assert!(matches!(failure.cause, flush_io::Cause::IdentityMismatch));
            no_io(&state);
            assert_eq!(inventory(&state), before);
            assert!(frame_test::flush_diagnose(flusher.pool).claim.is_some());
        });
        variant("BA08", name);
    }
}
fn associated_capture() {
    for (index, name) in [
        "allocator-policy/status-capture",
        "status-policy/allocator-capture",
        "prepared-class",
        "prepared-kind",
        "prepared-level",
    ]
    .into_iter()
    .enumerate()
    {
        fixture!(1, |bound, core, state| {
            let (mut writer, _, flusher) = bound.split_with_flush();
            let mut w = ok(core.try_owner(OwnerRole::Maintenance));
            initialize(&mut writer, &mut w, 0);
            let mut o = ok(core.try_owner(OwnerRole::DedicatedFlusher));
            let authority = ok(flusher.status_authority());
            let request = if index == 1 {
                ok(flusher.pool.target_request(0))
            } else {
                *authority.request()
            };
            let capture =
                ok(flusher
                    .pool
                    .capture_flush(&mut o, request, ok(flusher.control.io.image_mut())))
                .unwrap();
            let failure = if index == 0 {
                reset(&state);
                err(flusher.control.io.prepare(flusher.storage, &capture))
            } else if index == 1 {
                reset(&state);
                err(flusher
                    .control
                    .io
                    .prepare_status(flusher.storage, &capture, &authority))
            } else {
                ok(flusher
                    .control
                    .io
                    .prepare_status(flusher.storage, &capture, &authority));
                io_test::corrupt_target_association(&mut flusher.control.io, (index - 2) as u8);
                let proof = match ok(flusher.storage.wal().ensure_durable(&capture.requirement())) {
                    EnsureDurable::AlreadySynced(proof) | EnsureDurable::SyncedHere(proof) => proof,
                    _ => panic!("actual INIT is synced"),
                };
                reset(&state);
                err(flusher.control.io.execute_status(
                    flusher.storage,
                    &capture,
                    &authority,
                    &proof,
                ))
            };
            assert_eq!(failure.effect, Effect::CallerMustResolve);
            if index < 2 {
                assert!(matches!(failure.cause, flush_io::Cause::IdentityMismatch));
            } else {
                assert!(matches!(failure.cause, flush_io::Cause::Invariant));
            }
            no_io(&state);
            assert!(frame_test::flush_diagnose(flusher.pool).claim.is_some());
        });
        variant("BA09", name);
    }
}
pub(super) fn run_ba() {
    let mut count = 0;
    if selected("BA01") {
        for name in ["idle", "forgotten-Apply"] {
            ordinary_owner(name, true, OwnerRole::Foreground);
            variant("BA01", name);
        }
        passed("BA01", &mut count);
    }
    if selected("BA02") {
        for (name, role) in [
            ("foreground", OwnerRole::Foreground),
            ("maintenance", OwnerRole::Maintenance),
        ] {
            ordinary_owner(name, false, role);
            variant("BA02", name);
        }
        passed("BA02", &mut count);
    }
    if selected("BA03") {
        before_publication();
        passed("BA03", &mut count);
    }
    if selected("BA04") {
        convert_boundaries();
        passed("BA04", &mut count);
    }
    if selected("BA05") {
        bad_workspace();
        passed("BA05", &mut count);
    }
    if selected("BA06") {
        super::races::fenced_progress();
        passed("BA06", &mut count);
    }
    if selected("BA07") {
        both_clean();
        passed("BA07", &mut count);
    }
    if selected("BA08") {
        altered_policy();
        passed("BA08", &mut count);
    }
    if selected("BA09") {
        associated_capture();
        passed("BA09", &mut count);
    }
    if selected("BA10") {
        repeated_initialization();
        passed("BA10", &mut count);
    }
    complete("BA", count);
}
fn repeated_initialization() {
    fixture!(1, |bound, core, state| {
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let mut owner = ok(core.try_owner(OwnerRole::Maintenance));
        let reservation = ok(writer.reserve_one(&mut owner));
        let stale = (reservation.batch, reservation.page, reservation.ordinal);
        match reservation.initialize() {
            Ok(_) => {}
            _ => panic!("INIT"),
        }
        let mut o = ok(core.try_owner(OwnerRole::DedicatedFlusher));
        done(ok(flusher.try_flush_allocator(&mut o)));
        done(ok(flusher.try_flush_status(&mut o)));
        reset(&state);
        let before = inventory(&state);
        let error = err(writer.reserve_one(&mut owner));
        assert!(matches!(
            error.cause,
            super::super::super::Cause::AlreadyInitialized
        ));
        assert_eq!(error.effect, Effect::Unchanged);
        no_io(&state);
        variant("BA10", "reserve");
        // Test-only stale private authority request, not a callable second INIT.
        let error =
            err(writer.authority(stale.0, stale.1, stale.2, operation::mixed::Kind::Status));
        assert!(matches!(
            error.cause,
            super::super::super::Cause::AlreadyInitialized
        ));
        assert_eq!(error.effect, Effect::Unchanged);
        no_io(&state);
        assert_eq!(inventory(&state), before);
        variant("BA10", "initialize-private-stale-authority");
    });
    storage_fixture!(1, |storage, state| {
        let core = PageConcurrency::new(storage.session_fence()).unwrap();
        let mut a = boxed::<PAGE_BYTES>(0);
        let mut s = boxed::<PAGE_BYTES>(0);
        let mut work = boxed::<{ super::super::super::OPERATION_BYTES }>(0);
        let mut frames = [DirtyFrame::new(&mut a), DirtyFrame::new(&mut s)];
        {
            let mut bound = converted(
                ok(storage.bind_status_bootstrap(&core, &mut frames, &mut work)).into_with_flush(),
            );
            let (mut writer, _, mut flusher) = bound.split_with_flush();
            let mut owner = ok(core.try_owner(OwnerRole::Maintenance));
            initialize(&mut writer, &mut owner, 0);
            let mut o = ok(core.try_owner(OwnerRole::DedicatedFlusher));
            done(ok(flusher.try_flush_allocator(&mut o)));
            done(ok(flusher.try_flush_status(&mut o)));
        }
        storage_fixture!(1, |second, second_state| {
            let second_core = PageConcurrency::new(second.session_fence()).unwrap();
            let mut second_work = boxed::<{ super::super::super::OPERATION_BYTES }>(0);
            let failure =
                err(second.bind_status_bootstrap(&second_core, &mut frames, &mut second_work));
            assert_eq!(failure.effect, Effect::CallerMustResolve);
            assert!(matches!(
                failure.cause,
                super::super::super::super::Cause::Frame(_)
            ));
            no_io(&second_state);
        });
    });
    variant("BA10", "reused-wrappers");
}
