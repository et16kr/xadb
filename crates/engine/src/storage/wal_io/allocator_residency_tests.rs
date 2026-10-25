use super::*;
use crate::storage::page::PageLsn;
use crate::{
    storage::{
        dirty_frame::DirtyFrame,
        wal_io::{FreshStorageSession, bootstrap::test_support as port},
    },
    transaction::concurrency::{OwnerRole, PageConcurrency},
};
use foundation::codec::content_digest;

fn boxed<const N: usize>() -> Box<[u8; N]> {
    vec![0; N].into_boxed_slice().try_into().unwrap()
}
macro_rules! fixture {
    (|$bound:ident, $core:ident, $state:ident| $run:block) => {{
        let mut slot = port::slot();
        let mut workspace = boxed::<{ 4 << 20 }>();
        let mut page = boxed::<PAGE_BYTES>();
        let mut operation = boxed::<98304>();
        let (writer, $state) = port::fresh();
        let fresh =
            FreshStorageSession::create(writer, 1, &mut slot, &mut [[0; PAGE_BYTES]; 3]).unwrap();
        let installed = fresh.install_doublewrite(&mut workspace).unwrap();
        let storage = installed.into_mutable_storage();
        let $core = PageConcurrency::new(storage.session_fence()).unwrap();
        let mut frames = [DirtyFrame::new(&mut page)];
        let $bound = storage
            .bind_allocator(&$core, &mut frames, &mut operation)
            .unwrap();
        $run
    }};
}
fn operational<B>(
    result: std::result::Result<
        B,
        IntoOperationalError<BoundAllocation<'_, '_, '_, '_, '_, '_, '_, port::Writer>>,
    >,
) -> B {
    match result {
        Ok(bound) => bound,
        Err(IntoOperationalError::Before { error, .. })
        | Err(IntoOperationalError::Fatal(error)) => panic!("conversion: {error:?}"),
    }
}
fn flush<'s, 'c>(
    flusher: &mut AllocatorFlusher<'_, 's, 'c, '_, '_, '_, '_, port::Writer>,
    core: &'c PageConcurrency<'s>,
) {
    let mut owner = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
    assert!(matches!(
        flusher.try_flush_one(&mut owner).unwrap(),
        allocator_flush::FlushProgress::Complete(_)
    ));
}

#[test]
fn reload_layout_gate() {
    type W = port::Writer;
    let bound = size_of::<BoundAllocation<'_, '_, '_, '_, '_, '_, '_, W>>();
    let wrapper = size_of::<OperationalAllocation<'_, '_, '_, '_, '_, '_, '_, W>>();
    let result = size_of::<
        std::result::Result<
            OperationalAllocation<'_, '_, '_, '_, '_, '_, '_, W>,
            IntoOperationalError<BoundAllocation<'_, '_, '_, '_, '_, '_, '_, W>>,
        >,
    >();
    let reader = size_of::<PageFileReader<'_, W>>();
    let file = size_of::<<W as WriterNamespace>::File>();
    let conversion = bound + wrapper + result + 2048;
    let reload = 2 * reader + 4 * file + dirty_frame::CALL_BYTES + 4096 + 16384 + 4096;
    // Whole outer reload chain: returned/saved seals and stamps, both WAL
    // prefixes, copied request/permit/result, descriptor observations, and
    // reference/discriminant carriers. Nested reader/codec bounds are separate.
    let outer = 2 * size_of::<ReloadSnapshot<'_, '_>>()
        + 4 * size_of::<FrameStamp>()
        + 2 * size_of::<wal::WalPrefix>()
        + 2 * size_of::<dirty_frame::TargetRequest>()
        + 2 * size_of::<ReloadInstallPermit<'_, '_, '_>>()
        + 2 * size_of::<Result<ReloadObservation>>()
        + size_of::<ReloadObservation>()
        + 2 * size_of::<super::super::super::allocation_reservation::AllocationObservation>()
        + 32 * size_of::<&()>()
        + 128;
    let before = size_of::<(ReloadError, BoundAllocation<'_, '_, '_, '_, '_, '_, '_, W>)>();
    let error = size_of::<IntoOperationalError<BoundAllocation<'_, '_, '_, '_, '_, '_, '_, W>>>();
    assert!(outer <= 4096);
    assert!(before <= 21504 && error <= 21504);
    let (hot, cold) = super::super::shared_dirty_peaks();
    assert!(hot <= 196608 && cold <= 196608 && hot + 65536 <= 262144);
    println!(
        "ALLOCATOR_RELOAD_OUTER outer={outer} before={before} error={error} seal={} permit={} stamp={} observation={} result={} shared_hot={hot} shared_cold={cold}",
        size_of::<ReloadSnapshot<'_, '_>>(),
        size_of::<ReloadInstallPermit<'_, '_, '_>>(),
        size_of::<FrameStamp>(),
        size_of::<ReloadObservation>(),
        size_of::<Result<ReloadObservation>>()
    );
    println!(
        "ALLOCATOR_RELOAD_LAYOUT bound={bound} wrapper={wrapper} result={result} reader={reader} file={file} conversion={conversion} reload={reload} pool_delta={}",
        dirty_frame::init_pool_delta_bytes()
    );
    assert!(fits::<W>());
    #[cfg(target_os = "linux")]
    super::native_tests::assert_native_layout_gate();
}

#[test]
fn reload_preserves_active_reservation_and_resumes_cancel() {
    fixture!(|bound, core, state| {
        let mut bound = operational(bound.into_operational());
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let active;
        let descriptor;
        {
            let (mut writer, _, mut flusher) = bound.split_with_flush();
            writer.reserve_pages(&mut owner, 1).unwrap();
            active = writer.observe(&mut owner).unwrap();
            descriptor = writer.control.descriptor;
            flush(&mut flusher, &core);
        }
        let files = state
            .lock()
            .unwrap()
            .files
            .iter()
            .map(|(name, file)| (name.clone(), content_digest(&file.bytes)))
            .collect::<Vec<_>>();
        let reload = bound.reload_allocator().unwrap();
        assert_eq!(
            files,
            state
                .lock()
                .unwrap()
                .files
                .iter()
                .map(|(name, file)| (name.clone(), content_digest(&file.bytes)))
                .collect::<Vec<_>>()
        );
        assert_eq!((reload.before.generation, reload.after.generation), (1, 2));
        assert_eq!(
            (
                reload.after.page_lsn,
                reload.after.modification_counter,
                reload.after.rec_lsn
            ),
            (PageLsn(1), 1, None)
        );
        assert_eq!(bound.inner.writer.descriptor, descriptor);
        {
            let (mut writer, _, mut flusher) = bound.split_with_flush();
            let restored = writer.observe(&mut owner).unwrap();
            assert_eq!(restored.active, active.active);
            assert_eq!(restored.reserved, 1);
            assert_eq!(restored.work_high_water, active.work_high_water);
            writer.cancel_reserved(&mut owner).unwrap();
            flush(&mut flusher, &core);
        }
        let next = bound.reload_allocator().unwrap();
        assert_eq!((next.before.generation, next.after.generation), (2, 3));
        assert_eq!(next.after.page_lsn, PageLsn(2));
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let completed = writer.observe(&mut owner).unwrap();
        assert!(completed.active.is_none());
        assert_eq!(completed.reserved, 0);
        assert_eq!(completed.work_high_water, active.work_high_water);
        writer.reserve_pages(&mut owner, 1).unwrap();
        let resumed = writer.observe(&mut owner).unwrap();
        assert_eq!(resumed.work_high_water, active.work_high_water + 1);
        assert_eq!(resumed.reserved, 1);
        writer.cancel_reserved(&mut owner).unwrap();
        flush(&mut flusher, &core);
        let completed = writer.observe(&mut owner).unwrap();
        assert_eq!(completed.work_high_water, active.work_high_water + 1);
        assert!(completed.active.is_none());
        let stamp = image(&bound).1.frame;
        assert_eq!(
            (stamp.generation, stamp.page_lsn, stamp.rec_lsn),
            (3, PageLsn(4), None)
        );
    });
}

#[test]
fn reload_pristine_zero_lsn_keeps_clean_counter_zero() {
    fixture!(|bound, core, state| {
        let _ = (&core, &state);
        let mut bound = operational(bound.into_operational());
        let result = bound.reload_allocator().unwrap();
        assert_eq!((result.before.generation, result.after.generation), (1, 2));
        assert_eq!(
            (
                result.after.page_lsn,
                result.after.modification_counter,
                result.after.rec_lsn
            ),
            (PageLsn(0), 0, None)
        );
    });
}

#[test]
fn reload_dirty_refusal_has_no_source_reads() {
    fixture!(|bound, core, state| {
        let mut bound = operational(bound.into_operational());
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        {
            let (mut writer, _, _) = bound.split_with_flush();
            writer.reserve_pages(&mut owner, 1).unwrap();
        }
        let reads = state.lock().unwrap().reads;
        let descriptor = bound.inner.writer.descriptor;
        let error = bound.reload_allocator().unwrap_err();
        assert_eq!(error.effect, Effect::Unchanged);
        assert_eq!(state.lock().unwrap().reads, reads);
        assert_eq!(bound.inner.writer.descriptor, descriptor);
        assert!(!bound.inner.storage.session_fence().is_fenced());
    });
}

fn image(
    bound: &OperationalAllocation<'_, '_, '_, '_, '_, '_, '_, port::Writer>,
) -> (Box<[u8; PAGE_BYTES]>, dirty_frame::test_support::Snapshot) {
    let mut page = boxed::<PAGE_BYTES>();
    let snapshot = dirty_frame::test_support::diagnose(&bound.inner.pool, 0, &mut page);
    (page, snapshot)
}
fn same_frame(
    before: &(Box<[u8; PAGE_BYTES]>, dirty_frame::test_support::Snapshot),
    after: &(Box<[u8; PAGE_BYTES]>, dirty_frame::test_support::Snapshot),
) {
    assert_eq!(before.0, after.0);
    assert_eq!(before.1.frame, after.1.frame);
    assert_eq!(before.1.entry, after.1.entry);
    assert_eq!(
        (before.1.pins, before.1.total),
        (after.1.pins, after.1.total)
    );
}

#[test]
fn reload_pinned_refusal_preserves_counts_and_reads() {
    fixture!(|bound, core, state| {
        let mut bound = operational(bound.into_operational());
        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
        let request = bound.inner.pool.target_request(0).unwrap();
        let held = bound
            .inner
            .pool
            .reserve_targets(&mut owner, &[request])
            .unwrap();
        std::mem::forget(held); // Explicit orphan witness; never claim successful cleanup.
        let before = image(&bound);
        let reads = state.lock().unwrap().reads;
        let error = bound.reload_allocator().unwrap_err();
        assert_eq!(error.effect, Effect::Unchanged);
        same_frame(&before, &image(&bound));
        assert_eq!(state.lock().unwrap().reads, reads);
        assert!(!bound.inner.storage.session_fence().is_fenced());
    });
}

#[test]
fn reload_generation_exhaustion_precedes_source_reads() {
    fixture!(|bound, core, state| {
        let _ = &core;
        let mut bound = operational(bound.into_operational());
        dirty_frame::residency_test_support::seed_generation(&mut bound.inner.pool, u64::MAX);
        let before = image(&bound);
        let reads = state.lock().unwrap().reads;
        let error = bound.reload_allocator().unwrap_err();
        assert!(matches!(
            error.cause,
            Cause::Frame(dirty_frame::DirtyError {
                cause: dirty_frame::Cause::CounterExhausted,
                ..
            })
        ));
        assert_eq!(error.effect, Effect::Unchanged);
        same_frame(&before, &image(&bound));
        assert_eq!(state.lock().unwrap().reads, reads);
    });
}

#[test]
fn reload_read_failures_keep_authority_and_allow_explicit_retry() {
    // The mock's counter includes four opens as well as six actual reads.
    // Freeze their whole sequence independently of the product helper.
    let sites = [
        ("open", "d3-profile"),
        ("read", "d3-profile"),
        ("open", "FILEMAP"),
        ("read", "FILEMAP"),
        ("open", "d3-filemap-0"),
        ("read", "d3-filemap-0"),
        ("read", "d3-filemap-0"),
        ("read", "d3-filemap-0"),
        ("open", "d3-data-0000000100000001"),
        ("read", "d3-data-0000000100000001"),
    ];
    for (offset, (kind, name)) in sites.into_iter().enumerate() {
        fixture!(|bound, core, state| {
            let _ = &core;
            let mut bound = operational(bound.into_operational());
            let before = image(&bound);
            let descriptor = bound.inner.writer.descriptor;
            let start = state.lock().unwrap().reads;
            {
                let mut s = state.lock().unwrap();
                s.read_plan = Some(port::Plan {
                    at: start + offset,
                    action: port::Action::BeforeError,
                });
            }
            let error = bound.reload_allocator().unwrap_err();
            assert_eq!(error.effect, Effect::Unchanged);
            {
                let s = state.lock().unwrap();
                assert_eq!(s.hit, 1);
                let event = &s.read_events[start + offset];
                assert_eq!((event.kind, event.name.as_str()), (kind, name));
            }
            match error.cause {
                Cause::PageIo(PageIoError::Read(error)) if kind == "read" => {
                    assert_eq!(error.certainty(), foundation::error::Certainty::Known)
                }
                Cause::PageIo(PageIoError::Namespace(error)) if kind == "open" => {
                    assert_eq!(error.kind, NamespaceErrorKind::Io);
                    assert_eq!(error.effect, Effect::Unchanged);
                    assert_eq!(error.io_kind, Some(std::io::ErrorKind::Other));
                }
                other => panic!("original read cause missing: {other:?}"),
            }
            assert!(!bound.inner.storage.session_fence().is_fenced());
            same_frame(&before, &image(&bound));
            assert_eq!(bound.inner.writer.descriptor, descriptor);
            state.lock().unwrap().read_plan = None;
            let result = bound.reload_allocator().unwrap();
            assert_eq!((result.before.generation, result.after.generation), (1, 2));
        });
    }
}

#[test]
fn reload_source_mismatch_fences_without_replacing_ram() {
    for variant in 0..4 {
        fixture!(|bound, core, state| {
            let _ = &core;
            let mut bound = operational(bound.into_operational());
            let before = image(&bound);
            let home = "d3-data-0000000100000001";
            {
                let mut s = state.lock().unwrap();
                let entry = s.files.get_mut(home).unwrap();
                match variant {
                    0 => entry.bytes[0] ^= 1,
                    1 => {
                        entry.bytes[64..72].copy_from_slice(&1u64.to_le_bytes());
                    }
                    2 => {
                        entry.bytes[32..40].copy_from_slice(&1u64.to_le_bytes());
                    }
                    3 => {
                        // Literal slot0 offset and Free entry1 generation. A new
                        // valid Free generation keeps the envelope/LSN valid.
                        let slot = 128;
                        let cell =
                            u16::from_le_bytes(entry.bytes[slot..slot + 2].try_into().unwrap())
                                as usize;
                        let generation = cell + 64 + 96 + 8;
                        entry.bytes[generation..generation + 8]
                            .copy_from_slice(&2u64.to_le_bytes());
                    }
                    _ => unreachable!(),
                }
                if variant != 0 {
                    let digest = content_digest(&entry.bytes[..PAGE_BYTES - 32]);
                    entry.bytes[PAGE_BYTES - 32..PAGE_BYTES].copy_from_slice(&digest);
                }
            }
            let error = bound.reload_allocator().unwrap_err();
            assert_eq!(error.effect, Effect::CallerMustResolve);
            if variant == 3 {
                assert!(matches!(error.cause, Cause::HomeMismatch));
            }
            assert!(bound.inner.storage.session_fence().is_fenced());
            same_frame(&before, &image(&bound));
        });
    }
}

#[test]
fn reload_poison_fences_before_any_source_read() {
    for variant in 0..3 {
        fixture!(|bound, core, state| {
            let _ = &core;
            let mut bound = operational(bound.into_operational());
            let reads = state.lock().unwrap().reads;
            match variant {
                0 => dirty_frame::test_support::poison_content(&bound.inner.pool, 0),
                1 => dirty_frame::test_support::poison_dpt(&bound.inner.pool),
                2 => dirty_frame::test_support::poison_pins(&bound.inner.pool),
                _ => unreachable!(),
            }
            let error = bound.reload_allocator().unwrap_err();
            assert_eq!(error.effect, Effect::CallerMustResolve);
            assert!(bound.inner.storage.session_fence().is_fenced());
            assert_eq!(state.lock().unwrap().reads, reads);
        });
    }
}

#[test]
fn reload_old_seal_cannot_clear_a_new_dirty_epoch() {
    fixture!(|bound, core, state| {
        let _ = &state;
        let mut bound = operational(bound.into_operational());
        let mut scratch = boxed::<PAGE_BYTES>();
        let old = bound
            .inner
            .pool
            .prepare_allocator_reload(&mut scratch)
            .unwrap();
        bound.reload_allocator().unwrap();
        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
        {
            let (mut writer, _, _) = bound.split_with_flush();
            writer.reserve_pages(&mut owner, 1).unwrap();
        }
        let before = image(&bound);
        let error = bound
            .inner
            .pool
            .install_allocator_reload(ReloadInstallPermit {
                snapshot: &old,
                bytes: &scratch,
            })
            .unwrap_err();
        assert_eq!(error.effect, Effect::CallerMustResolve);
        assert!(bound.inner.storage.session_fence().is_fenced());
        same_frame(&before, &image(&bound));
        assert!(before.1.entry.is_some());
    });
}

#[test]
fn reload_conversion_returns_the_original_dirty_owner() {
    fixture!(|bound, core, state| {
        let mut bound = bound;
        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
        {
            let (mut writer, _) = bound.split();
            writer.reserve_pages(&mut owner, 1).unwrap();
        }
        let descriptor = bound.writer.descriptor;
        let reads = state.lock().unwrap().reads;
        let mut bound = match bound.into_operational() {
            Err(IntoOperationalError::Before { error, bound }) => {
                assert_eq!(error.effect, Effect::Unchanged);
                bound
            }
            _ => panic!("dirty conversion must return its original owner"),
        };
        assert_eq!(bound.writer.descriptor, descriptor);
        assert_eq!(state.lock().unwrap().reads, reads);
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        writer.cancel_reserved(&mut owner).unwrap();
        flush(&mut flusher, &core);
    });
}

#[test]
fn reload_namespace_structure_is_fatal_but_io_cause_is_preserved() {
    for effect in [Effect::Unchanged, Effect::CallerMustResolve] {
        for kind in [
            NamespaceErrorKind::Io,
            NamespaceErrorKind::NotFound,
            NamespaceErrorKind::InvalidType,
            NamespaceErrorKind::InvalidName,
        ] {
            fixture!(|bound, core, state| {
                let _ = (&core, &state);
                let bound = operational(bound.into_operational());
                let original = foundation::namespace::NamespaceError::new(
                    kind,
                    effect,
                    "fixed reload observation",
                );
                let error = bound.read_error(PageIoError::Namespace(original.clone()));
                assert_eq!(
                    error.effect,
                    if kind == NamespaceErrorKind::Io && effect == Effect::Unchanged {
                        Effect::Unchanged
                    } else {
                        Effect::CallerMustResolve
                    }
                );
                match error.cause {
                    Cause::PageIo(PageIoError::Namespace(error)) => assert_eq!(error, original),
                    other => panic!("lost namespace cause: {other:?}"),
                }
                assert_eq!(
                    bound.inner.storage.session_fence().is_fenced(),
                    kind != NamespaceErrorKind::Io || effect != Effect::Unchanged
                );
            });
        }
    }
}

#[test]
fn reload_lower_uncertain_read_is_never_downgraded() {
    fixture!(|bound, core, state| {
        let _ = (&core, &state);
        let bound = operational(bound.into_operational());
        let original = foundation::Error::new(
            foundation::error::ErrorCode::IoWriteUnknown,
            "lower uncertain read wrapper witness",
        );
        assert_eq!(original.effect(), Effect::CallerMustResolve);
        assert_eq!(original.certainty(), Certainty::Unknown);
        let error = bound.read_error(PageIoError::Read(original.clone()));
        assert_eq!(error.effect, Effect::CallerMustResolve);
        match error.cause {
            Cause::PageIo(PageIoError::Read(error)) => assert_eq!(error, original),
            other => panic!("lost original uncertain cause: {other:?}"),
        }
        assert!(bound.inner.storage.session_fence().is_fenced());
    });
}

#[test]
fn reload_older_valid_home_fences_without_replacing_ram() {
    fixture!(|bound, core, state| {
        let mut bound = operational(bound.into_operational());
        let home = "d3-data-0000000100000001";
        let pristine = state.lock().unwrap().files[home].bytes[..PAGE_BYTES].to_vec();
        {
            let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
            let (mut writer, _, mut flusher) = bound.split_with_flush();
            writer.reserve_pages(&mut owner, 1).unwrap();
            flush(&mut flusher, &core);
        }
        let before = image(&bound);
        assert_eq!(before.1.frame.page_lsn, PageLsn(1));
        state.lock().unwrap().files.get_mut(home).unwrap().bytes[..PAGE_BYTES]
            .copy_from_slice(&pristine);
        let error = bound.reload_allocator().unwrap_err();
        assert_eq!(error.effect, Effect::CallerMustResolve);
        assert!(bound.inner.storage.session_fence().is_fenced());
        same_frame(&before, &image(&bound));
    });
}

#[test]
fn reload_isolated_uncaptured_claim_refuses_before_reads() {
    fixture!(|bound, core, state| {
        let _ = &core;
        let mut bound = operational(bound.into_operational());
        dirty_frame::test_support::seed_uncaptured_claim(&bound.inner.pool);
        let before = image(&bound);
        assert!(before.1.entry.is_none());
        assert_eq!(before.1.total, 0);
        let claim = dirty_frame::test_support::flush_diagnose(&bound.inner.pool)
            .claim
            .unwrap();
        assert!(claim.captured.is_none());
        let reads = state.lock().unwrap().reads;
        let error = bound.reload_allocator().unwrap_err();
        assert_eq!(error.effect, Effect::Unchanged);
        same_frame(&before, &image(&bound));
        assert_eq!(state.lock().unwrap().reads, reads);
        assert_eq!(
            dirty_frame::test_support::flush_diagnose(&bound.inner.pool).claim,
            Some(claim)
        );
        assert!(!bound.inner.storage.session_fence().is_fenced());
    });
}

#[test]
fn reload_isolated_dpt_requirement_mismatch_fences_before_reads() {
    for requirement_only in [false, true] {
        fixture!(|bound, core, state| {
            let mut bound = operational(bound.into_operational());
            let (requirement, entry);
            {
                let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let (mut writer, _, mut flusher) = bound.split_with_flush();
                writer.reserve_pages(&mut owner, 1).unwrap();
                requirement = dirty_frame::test_support::actual_requirement(writer.pool, 0);
                entry =
                    dirty_frame::test_support::diagnose(writer.pool, 0, &mut boxed::<PAGE_BYTES>())
                        .entry;
                flush(&mut flusher, &core);
            }
            if requirement_only {
                dirty_frame::test_support::replace_requirement(
                    &bound.inner.pool,
                    0,
                    requirement.unwrap(),
                );
            } else {
                dirty_frame::test_support::seed_dpt(&bound.inner.pool, 0, entry);
            }
            let before = image(&bound);
            let reads = state.lock().unwrap().reads;
            let error = bound.reload_allocator().unwrap_err();
            assert_eq!(error.effect, Effect::CallerMustResolve);
            same_frame(&before, &image(&bound));
            assert_eq!(state.lock().unwrap().reads, reads);
            assert!(bound.inner.storage.session_fence().is_fenced());
        });
    }
}

#[test]
fn reload_isolated_writer_obligations_refuse_before_reads() {
    for installation in [false, true] {
        fixture!(|bound, core, state| {
            let mut bound = operational(bound.into_operational());
            let requirement;
            let stamp;
            {
                let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let (mut writer, _, mut flusher) = bound.split_with_flush();
                writer.reserve_pages(&mut owner, 1).unwrap();
                requirement =
                    dirty_frame::test_support::actual_requirement(writer.pool, 0).unwrap();
                stamp =
                    dirty_frame::test_support::diagnose(writer.pool, 0, &mut boxed::<PAGE_BYTES>())
                        .frame;
                flush(&mut flusher, &core);
            }
            if installation {
                bound.inner.writer.installation = Some(
                    dirty_allocation::test_retained_installation(requirement, stamp),
                );
            } else {
                bound.inner.writer.terminal_pending = true;
            }
            let before = image(&bound);
            assert!(before.1.entry.is_none());
            let reads = state.lock().unwrap().reads;
            let error = bound.reload_allocator().unwrap_err();
            assert!(matches!(error.cause, Cause::Busy));
            assert_eq!(error.effect, Effect::Unchanged);
            same_frame(&before, &image(&bound));
            assert_eq!(state.lock().unwrap().reads, reads);
            assert!(!bound.inner.storage.session_fence().is_fenced());
        });
    }
}

#[test]
fn reload_common_fence_during_home_read_keeps_registered_epoch() {
    fixture!(|bound, core, state| {
        let _ = &core;
        let mut bound = operational(bound.into_operational());
        let before = image(&bound);
        let storage = bound.inner.storage;
        let (entered, arrival) = std::sync::mpsc::sync_channel(0);
        let (release, resume) = std::sync::mpsc::sync_channel(0);
        {
            let mut s = state.lock().unwrap();
            s.read_pause = Some(port::Pause {
                at: s.reads + 9,
                after: false,
                entered,
                release: resume,
                unwind: false,
            });
        }
        std::thread::scope(|scope| {
            let worker = scope.spawn(move || {
                arrival
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap();
                storage.session_fence().fence();
                release.send(()).unwrap();
            });
            let error = bound.reload_allocator().unwrap_err();
            assert_eq!(error.effect, Effect::CallerMustResolve);
            worker.join().unwrap();
        });
        let event = state.lock().unwrap().read_events.last().unwrap().clone();
        assert_eq!(
            (event.kind, event.name.as_str()),
            ("read", "d3-data-0000000100000001")
        );
        same_frame(&before, &image(&bound));
        assert!(storage.session_fence().is_fenced());
    });
}

#[test]
fn reload_read_unwind_fences_and_keeps_old_registration() {
    for offset in [0, 9] {
        fixture!(|bound, core, state| {
            let _ = &core;
            let mut bound = operational(bound.into_operational());
            let before = image(&bound);
            {
                let mut s = state.lock().unwrap();
                s.read_plan = Some(port::Plan {
                    at: s.reads + offset,
                    action: port::Action::BeforePanic,
                });
            }
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| bound.reload_allocator()))
                    .is_err()
            );
            same_frame(&before, &image(&bound));
            assert!(bound.inner.storage.session_fence().is_fenced());
            assert_eq!(
                bound.reload_allocator().unwrap_err().effect,
                Effect::CallerMustResolve
            );
        });
    }
}

#[test]
fn reload_retired_pending_flush_refuses_before_reads() {
    fixture!(|bound, core, state| {
        let mut bound = operational(bound.into_operational());
        let mut reader_owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        {
            let request = bound.inner.pool.target_request(0).unwrap();
            let (mut writer, reader, mut flusher) = bound.split_with_flush();
            let mut writer_owner = core.try_owner(OwnerRole::Foreground).unwrap();
            writer.reserve_pages(&mut writer_owner, 1).unwrap();
            let (entered, arrival) = std::sync::mpsc::sync_channel(0);
            let (release, resume) = std::sync::mpsc::sync_channel(0);
            {
                let mut s = state.lock().unwrap();
                s.events.clear();
                s.pause = Some(port::Pause {
                    at: 30,
                    after: true,
                    entered,
                    release: resume,
                    unwind: false,
                });
            }
            let held = std::thread::scope(|scope| {
                let worker = scope.spawn(|| {
                    let mut owner = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                    flusher.try_flush_one(&mut owner).unwrap()
                });
                arrival
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap();
                let held = reader.read(&mut reader_owner, &request).unwrap();
                release.send(()).unwrap();
                assert_eq!(
                    worker.join().unwrap(),
                    allocator_flush::FlushProgress::RetiredPendingFinalize
                );
                held
            });
            std::mem::forget(held); // Deliberate retained pin; this fixture never claims cleanup.
        }
        let before = image(&bound);
        let reads = state.lock().unwrap().reads;
        let error = bound.reload_allocator().unwrap_err();
        assert!(matches!(error.cause, Cause::Busy));
        assert_eq!(error.effect, Effect::Unchanged);
        assert_eq!(state.lock().unwrap().reads, reads);
        same_frame(&before, &image(&bound));
    });
}

/// Expanded mixed-count history is selected only by final DEV3 S5 acceptance.
#[test]
#[ignore = "DEV3 S5 expanded allocator resident-epoch history"]
fn reload_s5_mixed_count_epoch_history() {
    fixture!(|bound, core, state| {
        let _ = &state;
        let mut bound = operational(bound.into_operational());
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        for round in 0..32u64 {
            let count = (round % 8 + 1) as usize;
            {
                let (mut writer, _, mut flusher) = bound.split_with_flush();
                writer.reserve_pages(&mut owner, count).unwrap();
                let active = writer.observe(&mut owner).unwrap();
                assert_eq!(active.reserved, count);
                assert_eq!(active.work_high_water, round + 1);
                flush(&mut flusher, &core);
            }
            let active = bound.reload_allocator().unwrap();
            assert_eq!(
                (active.after.generation, active.after.page_lsn),
                (2 * round + 2, PageLsn(2 * round + 1))
            );
            {
                let (mut writer, _, mut flusher) = bound.split_with_flush();
                let restored = writer.observe(&mut owner).unwrap();
                assert_eq!(restored.reserved, count);
                assert_eq!(restored.work_high_water, round + 1);
                writer.cancel_reserved(&mut owner).unwrap();
                flush(&mut flusher, &core);
            }
            let cancelled = bound.reload_allocator().unwrap();
            assert_eq!(
                (cancelled.after.generation, cancelled.after.page_lsn),
                (2 * round + 3, PageLsn(2 * round + 2))
            );
            assert_eq!(
                (
                    cancelled.after.modification_counter,
                    cancelled.after.rec_lsn
                ),
                (1, None)
            );
        }
    });
}
