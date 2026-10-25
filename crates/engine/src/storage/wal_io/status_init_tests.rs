//! Closed INIT cases consume independent literal bytes. Setup calls are fixture
//! construction, not reruns of the previously accepted bootstrap test selectors.
use super::*;
use crate::storage::{
    dirty_frame::{init_test_support as frame_init, test_support as frame_test},
    wal_io::{self as wal, FreshStorageSession, bootstrap::test_support as port},
};
use crate::transaction::concurrency::OwnerRole;
use foundation::codec::content_digest;
use foundation::identity::Generation;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

const INPUT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-status-init"
);
fn boxed<const N: usize>(byte: u8) -> Box<[u8; N]> {
    vec![byte; N].into_boxed_slice().try_into().unwrap()
}
fn literal(profile: u32, cancellations: usize, terminal: &str, name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{INPUT}/p{profile}-cancel{cancellations}-{terminal}/{name}"
    ))
    .unwrap()
}
fn selected(id: &str) -> bool {
    std::env::var("XADB_STATUS_INIT_CASES")
        .map_or(true, |value| value.split(',').any(|item| item == id))
}
fn mark(id: &str) {
    println!("XADB_STATUS_INIT_CASE {id}");
}
fn ok<T, E: std::fmt::Debug>(result: std::result::Result<T, E>) -> T {
    result.unwrap_or_else(|e| panic!("unexpected error {e:?}"))
}
fn err<T, E>(result: std::result::Result<T, E>) -> E {
    match result {
        Err(e) => e,
        Ok(_) => panic!("unexpected success"),
    }
}
fn init<W: WriterNamespace>(
    reserved: ReservedStatus<'_, '_, '_, '_, '_, '_, '_, '_, '_, W>,
) -> InitializedStatusObservation {
    match reserved.initialize() {
        Ok(observed) => observed,
        Err(InitAttemptError::Before(e)) => panic!("unexpected unchanged INIT {:?}", e.error),
        Err(InitAttemptError::Fatal(e)) => panic!("unexpected fatal INIT {e:?}"),
    }
}
fn cancel<W: WriterNamespace>(
    reserved: ReservedStatus<'_, '_, '_, '_, '_, '_, '_, '_, '_, W>,
) -> AllocationObservation {
    match reserved.cancel() {
        Ok(observed) => observed,
        Err(InitAttemptError::Before(e)) => panic!("unexpected unchanged cancel {:?}", e.error),
        Err(InitAttemptError::Fatal(e)) => panic!("unexpected fatal cancel {e:?}"),
    }
}
fn reset(state: &Arc<Mutex<port::State>>) {
    let mut state = state.lock().unwrap();
    state.events.clear();
    state.read_events.clear();
    state.reads = 0;
    state.hit = 0;
    state.peak = state.handles;
}
fn fixed_files(state: &Arc<Mutex<port::State>>) -> BTreeMap<String, (u64, [u8; 32])> {
    state
        .lock()
        .unwrap()
        .files
        .iter()
        .filter(|(name, _)| !name.starts_with("d3-wal-"))
        .map(|(name, entry)| {
            assert!(entry.sparse_tail.is_empty());
            (name.clone(), (entry.length, content_digest(&entry.bytes)))
        })
        .collect()
}
fn wal_bytes(state: &Arc<Mutex<port::State>>) -> Vec<u8> {
    state.lock().unwrap().files[wal::WAL_NAME].bytes.clone()
}
fn diagnose(
    pool: &SharedPool<'_, '_, '_, '_>,
    index: usize,
) -> (Box<[u8; PAGE_BYTES]>, frame_init::RawSnapshot) {
    let mut bytes = boxed::<PAGE_BYTES>(0);
    let observed = frame_init::snapshot(pool, index, &mut bytes);
    (bytes, observed)
}
macro_rules! storage_fixture {
    ($profile:expr, |$storage:ident, $state:ident| $body:block) => {{
        let mut slot = port::slot();
        let mut flush = boxed::<{4 << 20}>(0);
        let (namespace, $state) = port::fresh();
        let fresh = FreshStorageSession::create_status(namespace, $profile, &mut slot, &mut [[0; PAGE_BYTES]; 3]).unwrap();
        let installed = fresh.install_doublewrite(&mut flush).unwrap();
        let $storage = installed.into_mutable_storage();
        reset(&$state);
        $body
        drop($storage);
        let state = $state.lock().unwrap();
        assert_eq!((state.handles, state.writers), (0, 0));
    }};
}
macro_rules! fixture {
    ($profile:expr, |$bound:ident, $core:ident, $state:ident| $body:block) => {{
        storage_fixture!($profile, |storage, $state| {
            let $core = PageConcurrency::new(storage.session_fence()).unwrap();
            let mut allocator = boxed::<PAGE_BYTES>(0x2d);
            let mut payload = boxed::<PAGE_BYTES>(0xa5);
            let mut operation = boxed::<OPERATION_BYTES>(0x73);
            let mut frames = [DirtyFrame::new(&mut allocator), DirtyFrame::new(&mut payload)];
            let mut $bound = ok(storage.bind_status_bootstrap(&$core, &mut frames, &mut operation));
            let _ = &mut $bound;
            reset(&$state);
            $body
            drop($bound);
            assert!(storage.session_fence().is_fenced());
        });
    }};
}

#[path = "status_init_native_tests.rs"]
mod native;
#[path = "status_init_protocol_tests.rs"]
mod protocol;
#[path = "status_init_resource_tests.rs"]
mod resources;

#[test]
fn status_init_cf01_binding_preserves_closed_bytes() {
    for profile in 1..=3 {
        if !selected(&format!("CF01-{profile:02}")) {
            continue;
        }
        fixture!(profile, |bound, core, state| {
            let (_, closed) = diagnose(&bound.pool, 1);
            let (payload, _) = diagnose(&bound.pool, 1);
            assert!(payload.iter().all(|byte| *byte == 0xa5));
            assert_eq!(closed.page_lsn, PageLsn(0));
            assert_eq!(closed.counter, 0);
            assert_eq!(closed.rec_lsn, None);
            assert!(!closed.dirty);
            assert!(bound.pool.initialized_request().unwrap().is_none());
            assert!(bound.storage.workspace.lock().unwrap().is_some());
            assert!(state.lock().unwrap().events.is_empty());
            assert!(core.uses_fence(bound.storage.session_fence()));
            mark(&format!("CF01-{profile:02}"));
        });
    }
}

#[test]
fn status_init_cf02_unpublished_has_no_reader_admission() {
    fixture!(1, |bound, core, state| {
        let original = fixed_files(&state);
        let (mut writer, reader) = bound.split();
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        if selected("CF02-01") {
            let rejected = err(reader.read(&mut owner));
            assert_eq!(rejected.effect, Effect::Unchanged);
            assert!(matches!(rejected.cause, Cause::NotInitialized));
            assert_eq!(diagnose(writer.pool, 1).1.total, 0);
            mark("CF02-01");
        }
        if selected("CF02-03") {
            let observation = writer.allocator().observe(&mut owner).unwrap();
            assert_eq!(observation.reserved, 0);
            assert_eq!(observation.page_lsn, PageLsn(0));
            mark("CF02-03");
        }
        if selected("CF02-02") {
            // Ordinary lookup cannot acquire the unpublished physical slot.
            let request = TargetRequest {
                identity: PageIdentity {
                    page: PageRef {
                        page_no: 1,
                        incarnation: Generation::new(2).unwrap(),
                        ..writer.base.identity.page
                    },
                    owner: writer.role.object(),
                    owner_generation: writer.role.generation(),
                    ..writer.base.identity
                },
                kind: PageKind::TransactionStatus,
                level: 0,
            };
            let rejected = err(writer.pool.reserve_targets(&mut owner, &[request]));
            assert_eq!(rejected.effect, Effect::Unchanged);
            assert_eq!(rejected.cause, dirty_frame::Cause::MissingTarget);
            let mut flush_owner = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
            let mut capture = boxed::<PAGE_BYTES>(0x6c);
            let before_attempt = frame_test::flush_diagnose(writer.pool).last_attempt;
            let rejected = err(writer
                .pool
                .capture_flush(&mut flush_owner, request, &mut capture));
            assert_eq!(rejected.cause, dirty_frame::Cause::MissingTarget);
            assert_eq!(rejected.effect, Effect::Unchanged);
            assert_eq!(
                frame_test::flush_diagnose(writer.pool).last_attempt,
                before_attempt
            );
            assert!(capture.iter().all(|byte| *byte == 0x6c));
            assert_eq!(fixed_files(&state), original);
            assert!(state.lock().unwrap().events.is_empty());
            mark("CF02-02");
        }
    });
}

fn assert_history(
    writer: &StatusBootstrapWriter<'_, '_, '_, '_, '_, '_, '_, '_, port::Writer>,
    state: &Arc<Mutex<port::State>>,
    profile: u32,
    cancellations: usize,
    terminal: &str,
) {
    let work = cancellations + 1;
    let (allocator, a) = diagnose(writer.pool, 0);
    assert_eq!(
        &allocator[..],
        literal(
            profile,
            cancellations,
            terminal,
            &format!("work{work}-allocator.after")
        )
    );
    assert_eq!(
        &writer.base.descriptor[..],
        literal(
            profile,
            cancellations,
            terminal,
            &format!("work{work}-{terminal}.descriptor")
        )
    );
    assert_eq!(
        &wal_bytes(state)[4096..],
        literal(profile, cancellations, terminal, "wal.frames-only")
    );
    assert_eq!(a.page_lsn, PageLsn(2 * work as u64));
    assert_eq!(a.rec_lsn, Some(PageLsn(1)));
    assert_eq!(a.total, 0);
    assert!(writer.base.batch.is_none());
    assert!(!writer.base.terminal_pending);
    assert!(writer.installation.is_none());
    if terminal == "init" {
        let (status, s) = diagnose(writer.pool, 1);
        assert_eq!(
            &status[..],
            literal(
                profile,
                cancellations,
                terminal,
                &format!("work{work}-status.after")
            )
        );
        assert_eq!(s.page_lsn, a.page_lsn);
        assert_eq!(s.rec_lsn, Some(a.page_lsn));
        assert_eq!(s.counter, 1);
        assert!(s.dirty);
        assert!(writer.pool.initialized_request().unwrap().is_some());
    }
}

#[test]
fn status_init_cf07_profiles_publish_full_literal_images() {
    for profile in 1..=3 {
        if !selected(&format!("CF07-{profile:02}")) {
            continue;
        }
        fixture!(profile, |bound, core, state| {
            let homes = fixed_files(&state);
            let (mut writer, reader) = bound.split();
            let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
            let reservation = writer.reserve_one(&mut owner).unwrap();
            assert_eq!(reservation.page().incarnation, Generation::new(2).unwrap());
            let observed = init(reservation);
            assert_eq!(observed.lsn, PageLsn(2));
            assert_eq!(observed.wal.complete_lsn, observed.wal.synced_lsn);
            assert_history(&writer, &state, profile, 0, "init");
            let guard = reader.read(&mut owner).unwrap();
            assert_eq!(
                &guard.bytes().unwrap()[..],
                literal(profile, 0, "init", "work1-status.after")
            );
            guard.release_checked().unwrap();
            assert_eq!(fixed_files(&state), homes);
            mark(&format!("CF07-{profile:02}"));
        });
    }
}

#[test]
fn status_init_cf15_initialized_and_reused_binding_rejected() {
    if selected("CF15-01") || selected("CF15-02") {
        fixture!(1, |bound, core, state| {
            let (mut writer, _) = bound.split();
            let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
            init(writer.reserve_one(&mut owner).unwrap());
            let saved = wal_bytes(&state);
            if selected("CF15-02") {
                let rejected = err(writer.reserve_one(&mut owner));
                assert_eq!(rejected.effect, Effect::Unchanged);
                assert!(matches!(rejected.cause, Cause::AlreadyInitialized));
                mark("CF15-02");
            }
            if selected("CF15-01") {
                let rejected = err(writer.authority(
                    Batch {
                        owner: SystemWorkRef::new(
                            writer.storage.profile().spec().namespace,
                            crate::storage::allocation_page::SystemWorkKind::AllocationMaintenance,
                            1,
                            Generation::new(1).unwrap(),
                        )
                        .unwrap(),
                        mask: 2,
                        birth: 1,
                    },
                    PageRef {
                        page_no: 1,
                        incarnation: Generation::new(2).unwrap(),
                        ..writer.base.identity.page
                    },
                    0,
                    operation::mixed::Kind::Status,
                ));
                assert_eq!(rejected.effect, Effect::Unchanged);
                assert!(matches!(rejected.cause, Cause::AlreadyInitialized));
                assert_eq!(wal_bytes(&state), saved);
                mark("CF15-01");
            }
        });
    }
    if selected("CF15-03") {
        storage_fixture!(1, |storage, state| {
            let core = PageConcurrency::new(storage.session_fence()).unwrap();
            let mut allocator = boxed::<PAGE_BYTES>(0);
            let mut payload = boxed::<PAGE_BYTES>(0);
            let mut work = boxed::<OPERATION_BYTES>(0);
            let mut frames = [
                DirtyFrame::new(&mut allocator),
                DirtyFrame::new(&mut payload),
            ];
            {
                let mut bound = storage
                    .bind_status_bootstrap(&core, &mut frames, &mut work)
                    .unwrap();
                let (mut writer, _) = bound.split();
                let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
                init(writer.reserve_one(&mut owner).unwrap());
            }
            storage_fixture!(1, |second, second_state| {
                let second_core = PageConcurrency::new(second.session_fence()).unwrap();
                let mut second_work = boxed::<OPERATION_BYTES>(0);
                let failure =
                    err(second.bind_status_bootstrap(&second_core, &mut frames, &mut second_work));
                assert_eq!(failure.effect, Effect::CallerMustResolve);
                assert!(matches!(failure.cause, StorageCause::Frame(_)));
                assert!(second_state.lock().unwrap().events.is_empty());
                assert!(second_state.lock().unwrap().read_events.is_empty());
            });
            assert!(state.lock().unwrap().files.contains_key(wal::WAL_NAME));
            mark("CF15-03");
        });
    }
}

#[test]
fn status_init_cf16_retry_and_cancel_keep_exact_history() {
    for terminal in ["init", "cancel"] {
        if !selected(if terminal == "init" {
            "CF16-01"
        } else {
            "CF16-02"
        }) {
            continue;
        }
        fixture!(3, |bound, core, state| {
            let (mut writer, _) = bound.split();
            let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
            let mut first = writer.reserve_one(&mut owner).unwrap();
            let first_page = first.page();
            if terminal == "init" {
                first.ordinal = 1;
                let InitAttemptError::Before(rejected) = err(first.initialize()) else {
                    panic!("healthy rejection lost token")
                };
                assert_eq!(rejected.error.effect, Effect::Unchanged);
                first = rejected.reservation;
                first.ordinal = 0;
                assert_eq!(first.page(), first_page);
            }
            cancel(first);
            let second = writer.reserve_one(&mut owner).unwrap();
            assert_eq!(
                second.page().incarnation.get(),
                first_page.incarnation.get() + 1
            );
            if terminal == "init" {
                init(second);
            } else {
                cancel(second);
            }
            assert_history(&writer, &state, 3, 1, terminal);
            mark(if terminal == "init" {
                "CF16-01"
            } else {
                "CF16-02"
            });
        });
    }
}

#[test]
#[allow(
    clippy::result_large_err,
    reason = "observe the real inline reservation-return API without changing its allocation or unwind behavior"
)]
fn status_init_cf10_wal_failures_retain_installed_obligation() {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    for (case, at, action, panics, frame_len) in [
        (1, 1, port::Action::BeforeError, false, 38000),
        (2, 1, port::Action::AfterError, false, 38000),
        (3, 1, port::Action::AfterPanic, true, 38000),
        (4, 0, port::Action::PrefixError, false, 19000),
    ] {
        if !selected(&format!("CF10-{case:02}")) {
            continue;
        }
        fixture!(1, |bound, core, state| {
            let homes = fixed_files(&state);
            let (mut writer, _) = bound.split();
            let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
            {
                let reservation = writer.reserve_one(&mut owner).unwrap();
                reset(&state);
                state.lock().unwrap().plan = Some(port::Plan { at, action });
                let attempt = catch_unwind(AssertUnwindSafe(|| reservation.initialize()));
                match attempt {
                    Err(_) => assert!(panics),
                    Ok(Err(InitAttemptError::Fatal(error))) => {
                        assert!(!panics);
                        assert_eq!(error.effect, Effect::CallerMustResolve);
                        assert_eq!(error.phase, Phase::Sync);
                        let Cause::Wal(wal_error) = error.cause else {
                            panic!("original WAL cause lost")
                        };
                        assert_eq!(wal_error.effect, Effect::CallerMustResolve);
                        assert_eq!(
                            wal_error.phase,
                            if case == 4 {
                                wal::Phase::Write
                            } else {
                                wal::Phase::Sync
                            }
                        );
                        let wal::Cause::Io(lower) = wal_error.cause else {
                            panic!("original lower I/O cause lost")
                        };
                        assert_eq!(lower.code, foundation::error::ErrorCode::IoWriteUnknown);
                        assert_eq!(
                            lower.context,
                            if case == 4 {
                                "write outcome requires caller resolution"
                            } else {
                                "sync outcome unknown"
                            }
                        );
                        assert_eq!(lower.io_kind, Some(std::io::ErrorKind::Other));
                        assert_eq!(lower.effect(), Effect::CallerMustResolve);
                        assert_eq!(lower.certainty(), foundation::error::Certainty::Unknown);
                        assert_eq!(
                            lower.retry_scope(),
                            foundation::error::RetryScope::AfterRecovery
                        );
                    }
                    _ => panic!("fault must not succeed or return reservation"),
                }
            }
            assert!(writer.storage.session_fence().is_fenced());
            assert!(writer.installation.is_some());
            assert!(writer.base.terminal_pending && writer.base.batch.is_some());
            assert_eq!(
                writer.installation.as_ref().unwrap().phase,
                InitInstallPhase::Published
            );
            let (allocator, a) = diagnose(writer.pool, 0);
            let (status, s) = diagnose(writer.pool, 1);
            assert_eq!(
                &allocator[..],
                literal(1, 0, "init", "work1-allocator.after")
            );
            assert_eq!(&status[..], literal(1, 0, "init", "work1-status.after"));
            assert!(a.claim_present && s.claim_present);
            assert!(!a.initializer_pins_held);
            assert_eq!(a.total, 0);
            assert_eq!(s.total, 0);
            let bytes = wal_bytes(&state);
            let expected = literal(1, 0, "init", "wal.frames-only");
            assert_eq!(bytes.len(), 4096 + 21200 + frame_len);
            assert_eq!(&bytes[4096..], &expected[..21200 + frame_len]);
            assert_eq!(fixed_files(&state), homes);
            assert_eq!(state.lock().unwrap().hit, 1);
            println!(
                "XADB_STATUS_INIT_WAL_FAILURE CF10-{case:02} actual_frame_bytes={frame_len} sync_partial_bytes=not_applicable"
            );
            mark(&format!("CF10-{case:02}"));
        });
    }
}

#[test]
fn status_init_cf17_actual_resource_and_capacity_bounds() {
    if selected("CF17-01") {
        type W = port::Writer;
        let pool_delta = dirty_frame::init_pool_delta_bytes();
        let control = size_of::<StatusControl<'_, '_, '_>>();
        let control_delta = control.saturating_sub(dirty_allocation::baseline_control_bytes());
        for (name, actual, limit) in [
            ("pool_delta", pool_delta, dirty_frame::INIT_POOL_DELTA_LIMIT),
            ("base_delta", dirty_allocation::base_delta(), 64),
            ("control_delta", control_delta, 1536),
            ("plans", operation::plan_bytes(), 4096),
            ("temporaries", operation::temporary_bytes(), 16384),
            ("status_codec", operation::status_codec_bytes(), 8192),
            ("reader", operation::reader_bytes(), 3648),
            ("obligation", size_of::<InitObligation<'_, '_>>(), 768),
            ("permit", size_of::<InitInstallPermit<'_, '_, '_>>(), 768),
            ("upper_caller", init_caller_bytes::<W>(), 768),
            (
                "final_observation_join",
                size_of::<InitializedStatusObservation>()
                    + size_of::<Result<InitializedStatusObservation>>(),
                768,
            ),
            (
                "permit_commit_join",
                size_of::<InitInstallPermit<'_, '_, '_>>()
                    + size_of::<Result<InitInstallPermit<'_, '_, '_>>>()
                    + size_of::<InitControlCommit<'_>>(),
                768,
            ),
            (
                "reservation",
                size_of::<ReservedStatus<'_, '_, '_, '_, '_, '_, '_, '_, '_, W>>(),
                256,
            ),
            (
                "attempt_error",
                size_of::<InitAttemptError<ReservedStatus<'_, '_, '_, '_, '_, '_, '_, '_, '_, W>>>(
                ),
                512,
            ),
            (
                "bound",
                size_of::<BoundStatusBootstrap<'_, '_, '_, '_, '_, '_, '_, W>>(),
                28672,
            ),
            (
                "bound_result",
                size_of::<super::super::Result<BoundStatusBootstrap<'_, '_, '_, '_, '_, '_, '_, W>>>(
                ),
                28672,
            ),
            (
                "dirty_hot",
                60672 + pool_delta + control_delta + 34 * READ_CALL + INIT_EXCLUSIVE,
                196608,
            ),
            (
                "dirty_cold",
                60672
                    + pool_delta
                    + control_delta
                    + 90112
                    + (5 * pool_delta + 2048).max(3 * pool_delta + 8 * control_delta + 2048),
                196608,
            ),
        ] {
            println!("XADB_STATUS_INIT_RESOURCE {name} {actual} {limit}");
            assert!(actual <= limit, "{name}: {actual}>{limit}");
        }
        let limits = [
            dirty_frame::INIT_POOL_DELTA_LIMIT,
            256,
            256,
            512,
            128,
            256,
            256,
            640,
            640,
            640,
            640,
            1024,
            1024,
            64,
            512,
        ];
        for ((name, size), limit) in frame_init::init_layout().into_iter().zip(limits) {
            println!("XADB_STATUS_INIT_NATIVE_LAYOUT {name} {size} {limit}");
            assert!(size <= limit, "{name}: {size}>{limit}");
        }
        assert!(fits::<W>());
        mark("CF17-01");
    }
    if selected("CF17-04") {
        fixture!(1, |bound, core, state| {
            let (mut writer, _) = bound.split();
            let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
            // 50,000 bytes would fit the old pair of 21,200-byte frames. The
            // actual Status reservation must protect 21,200+38,000 bytes instead.
            {
                let mut append = writer.storage.wal().append.lock().unwrap();
                append.prefix.complete_offset = wal::WAL_FILE_CAP - 50000;
                append.prefix.synced_offset = append.prefix.complete_offset;
            }
            let saved = wal_bytes(&state);
            let rejected = err(writer.reserve_one(&mut owner));
            assert_eq!(rejected.effect, Effect::Unchanged);
            assert!(matches!(
                rejected.cause,
                Cause::Allocation(dirty_allocation::DirtyAllocationError {
                    cause: dirty_allocation::Cause::Capacity,
                    ..
                })
            ));
            assert_eq!(wal_bytes(&state), saved);
            assert!(state.lock().unwrap().events.is_empty());
            assert!(writer.base.batch.is_none());
            assert!(!writer.storage.session_fence().is_fenced());
            mark("CF17-04");
        });
    }
}

// Independent bounded witnesses for the adopted handoff contract. These
// synthetic checker inputs do not replace actual CF08/CF12 interleavings.
#[derive(Clone)]
struct InitWitness {
    publication_last: bool,
    whole_precheck: bool,
    retained_obligation: bool,
    closed_ordinary_admission: bool,
    receipt_same_group: bool,
    pin_lock_under_content: bool,
    claim_return_after_durable_observation: bool,
    allocator_membership_checked: bool,
}
fn inspect_witness(w: &InitWitness) -> std::result::Result<(), &'static str> {
    if !w.publication_last {
        return Err("early_publication");
    }
    if !w.whole_precheck {
        return Err("partial_precheck");
    }
    if !w.retained_obligation {
        return Err("lost_obligation");
    }
    if w.closed_ordinary_admission {
        return Err("unpublished_admission");
    }
    if !w.receipt_same_group {
        return Err("wrong_receipt");
    }
    if w.pin_lock_under_content {
        return Err("content_to_pin_lock");
    }
    if !w.claim_return_after_durable_observation {
        return Err("early_claim_return");
    }
    if !w.allocator_membership_checked {
        return Err("missing_membership");
    }
    Ok(())
}
#[test]
fn status_init_cf18_known_bad_witnesses() {
    if !selected("CF18-01") {
        return;
    }
    let valid = InitWitness {
        publication_last: true,
        whole_precheck: true,
        retained_obligation: true,
        closed_ordinary_admission: false,
        receipt_same_group: true,
        pin_lock_under_content: false,
        claim_return_after_durable_observation: true,
        allocator_membership_checked: true,
    };
    assert_eq!(inspect_witness(&valid), Ok(()));
    let expected = [
        "early_publication",
        "partial_precheck",
        "lost_obligation",
        "unpublished_admission",
        "wrong_receipt",
        "content_to_pin_lock",
        "early_claim_return",
        "missing_membership",
    ];
    for (case, reason) in expected.into_iter().enumerate() {
        let mut broken = valid.clone();
        match case {
            0 => broken.publication_last = false,
            1 => broken.whole_precheck = false,
            2 => broken.retained_obligation = false,
            3 => broken.closed_ordinary_admission = true,
            4 => broken.receipt_same_group = false,
            5 => broken.pin_lock_under_content = true,
            6 => broken.claim_return_after_durable_observation = false,
            7 => broken.allocator_membership_checked = false,
            _ => unreachable!(),
        }
        assert_eq!(inspect_witness(&broken), Err(reason));
    }
    println!(
        "XADB_STATUS_INIT_ORACLE synthetic_valid=1 rejected_known_bad=8 runtime_interleaving=false"
    );
    mark("CF18-01");
}

#[test]
#[ignore = "DEV3 S5 expanded cancellation histories only"]
fn status_init_s5_retry_histories() {
    for profile in 1..=3 {
        for cancellations in [1, 2, 8] {
            for terminal in ["init", "cancel"] {
                // These exact two histories are mandatory CF16 aliases.
                if profile == 3 && cancellations == 1 {
                    continue;
                }
                let id = format!("SR-P{profile}-C{cancellations}-{}", terminal.to_uppercase());
                if !selected(&id) {
                    continue;
                }
                fixture!(profile, |bound, core, state| {
                    let (mut writer, reader) = bound.split();
                    let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
                    for cycle in 0..cancellations {
                        let reserved = writer.reserve_one(&mut owner).unwrap();
                        assert_eq!(reserved.page().incarnation.get(), cycle as u64 + 2);
                        cancel(reserved);
                    }
                    let reserved = writer.reserve_one(&mut owner).unwrap();
                    if terminal == "init" {
                        init(reserved);
                    } else {
                        cancel(reserved);
                    }
                    assert_history(&writer, &state, profile, cancellations, terminal);
                    if terminal == "init" {
                        reader.read(&mut owner).unwrap().release_checked().unwrap();
                    } else {
                        assert!(matches!(
                            err(reader.read(&mut owner)).cause,
                            Cause::NotInitialized
                        ));
                    }
                });
                println!("XADB_STATUS_INIT_S5_CASE {id}");
            }
        }
    }
}
