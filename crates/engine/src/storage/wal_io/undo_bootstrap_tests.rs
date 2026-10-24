//! New plan3 branch only; seeded roles do not prove successful installation.
use super::*;

#[path = "undo_bootstrap_native_tests.rs"]
mod native_undo;

const UNDO_INPUT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-undo-bootstrap"
);
fn mark_undo(id: &str) {
    println!("\nXADB_UNDO_BOOTSTRAP_CASE {id}");
}
fn undo_literal(profile: u32) -> Vec<u8> {
    std::fs::read(format!("{UNDO_INPUT}/profile{profile}-d3-bootstrap.bin")).unwrap()
}
fn undo_plan(profile: u32, scratch: &mut [[u8; PAGE_BYTES]; 3]) -> boot::BootstrapPlan {
    let old = prepared(profile, false, scratch);
    boot::BootstrapPlan::prepare_for_kind(
        old.profile,
        scratch,
        boot::BootstrapPlanKind::UndoBootstrap,
    )
    .unwrap()
}
fn undo_seed<'m>(
    slot: &'m mut [u8; wal::MAX_FRAME_BYTES],
) -> (boot::FreshStorageSession<'m, port::Writer>, State) {
    let (mut fresh, state) = seed(slot, false);
    fresh.plan = undo_plan(1, &mut [[0; PAGE_BYTES]; 3]);
    (fresh, state)
}
fn undo_installed<'m, 'w>(
    slot: &'m mut [u8; wal::MAX_FRAME_BYTES],
    workspace: &'w mut [u8; super::super::super::WORKSPACE_BYTES],
) -> (DoublewriteStorageSession<'m, 'w, port::Writer>, State) {
    let (fresh, state) = undo_seed(slot);
    let plan = super::super::super::InstallPlan::prepare(&fresh.plan, workspace).unwrap();
    (
        DoublewriteStorageSession {
            fresh,
            _workspace: workspace,
            _plan: plan,
        },
        state,
    )
}
fn undo_exact(role: boot::UndoRole<'_>, fence: &SessionFence) {
    assert_eq!(role.namespace(), port::namespace());
    assert_eq!(role.object().get(), 3);
    assert_eq!(role.generation().get(), 1);
    assert_eq!(role.kind(), PageKind::Undo);
    assert_eq!(role.level(), 0);
    assert_eq!(role.issued_object_high_water(), 3);
    assert!(role.uses_fence(fence));
    assert!(!role.uses_fence(&SessionFence::new()));
}

#[test]
#[allow(
    clippy::manual_bits,
    reason = "eight source-owned scalar slots, counted in bytes"
)]
fn undo_bootstrap_ub01_literals_and_layout() {
    for profile in 1..=3 {
        let mut scratch = [[0xa5; PAGE_BYTES]; 3];
        let plan = undo_plan(profile, &mut scratch);
        let expected = undo_literal(profile);
        assert_eq!(&scratch[2][..4096], expected);
        assert_eq!(plan.decode_descriptor(&expected), Ok(()));
        let mut output = [0xa5; 4097];
        assert_eq!(plan.encode_descriptor(&mut output).unwrap(), expected);
        assert_eq!(output[4096], 0xa5);
        let old = literal(profile, true);
        for at in 0..4064 {
            if !(120..128).contains(&at) && !(280..288).contains(&at) {
                assert_eq!(expected[at], old[at], "field {at}");
            }
        }
        mark_undo(&format!("UB01-{profile:02}"));
    }
    type UndoResult = std::result::Result<boot::UndoRole<'static>, boot::BootstrapError>;
    type MutableResult = Result<boot::UndoRole<'static>>;
    let budget = boot::source_budget::<port::Writer>();
    // Independently sum the new nested branch and wrappers. Existing bootstrap
    // prepare/readback formulas are unchanged, not re-counted as new test cases.
    let undo = budget.fresh_role
        + 2 * size_of::<boot::UndoRole<'_>>()
        + 3 * size_of::<UndoResult>()
        + size_of::<boot::BootstrapError>()
        + 8 * size_of::<usize>();
    let mutable = undo
        + 2 * size_of::<MutableStorageError>()
        + 2 * size_of::<MutableResult>()
        + size_of::<boot::BootstrapError>()
        + 12 * size_of::<usize>();
    assert_eq!(budget.undo_role, undo);
    assert_eq!(budget.mutable_undo_role, mutable);
    assert!(undo <= 1024 && mutable <= 2048);
    assert!(size_of::<boot::UndoRole<'_>>() <= 32);
    assert_eq!(
        size_of::<boot::UndoRole<'_>>(),
        size_of::<boot::StatusRole<'_>>()
    );
    assert!(boot::source_budget_fits::<port::Writer>());
    assert!(boot::source_budget_fits::<super::native::Writer>());
    println!(
        "XADB_UNDO_BOOTSTRAP_LAYOUT role={} result={} mutable_result={} fresh={} mutable={} memory_admit=true native_admit=true",
        size_of::<boot::UndoRole<'_>>(),
        size_of::<UndoResult>(),
        size_of::<MutableResult>(),
        undo,
        mutable
    );
    mark_undo("UB01-04");
}

#[test]
fn undo_bootstrap_ub02_decoder() {
    let plan = undo_plan(1, &mut [[0; PAGE_BYTES]; 3]);
    let mut case = 0;
    for wire in [1, 2, 0, 4, u64::MAX] {
        assert_eq!(
            plan.decode_descriptor(&changed(undo_literal(1), 280, wire)),
            Err(ControlError::UnsupportedFormat)
        );
        case += 1;
        mark_undo(&format!("UB02-{case:02}"));
    }
    for high in [0, 1, 2, u64::MAX] {
        assert_eq!(
            plan.decode_descriptor(&changed(undo_literal(1), 120, high)),
            Err(ControlError::Corrupt)
        );
        case += 1;
        mark_undo(&format!("UB02-{case:02}"));
    }
    let mut raw = changed(undo_literal(1), 280, 4);
    raw[4064] ^= 1;
    assert_eq!(plan.decode_descriptor(&raw), Err(ControlError::Corrupt));
    mark_undo("UB02-10");
    let mut raw = undo_literal(1);
    raw[288] = 1;
    reseal(&mut raw);
    assert_eq!(plan.decode_descriptor(&raw), Err(ControlError::Corrupt));
    mark_undo("UB02-11");
    assert_eq!(
        plan.decode_descriptor(&undo_literal(2)),
        Err(ControlError::IdentityMismatch)
    );
    mark_undo("UB02-12");
}

#[test]
fn undo_bootstrap_ub03_role_health() {
    for case in 1..=5 {
        let mut slot = port::slot();
        let (fresh, state) = match case {
            2 => seed(&mut slot, false),
            3 => seed(&mut slot, true),
            _ => undo_seed(&mut slot),
        };
        if case >= 4 {
            fresh.wal.fenced.fence();
        }
        if case == 5 {
            poison(&fresh.wal._writer);
        }
        match case {
            1 => {
                undo_exact(fresh.undo_role().unwrap(), &fresh.wal.fenced);
                let status = fresh.status_role().unwrap();
                assert_eq!(status.object().get(), 2);
                assert_eq!(status.generation().get(), 1);
                assert_eq!(status.kind(), PageKind::TransactionStatus);
                assert_eq!(status.issued_object_high_water(), 3);
                assert_eq!(status.namespace(), port::namespace());
                assert!(status.uses_fence(&fresh.wal.fenced));
            }
            2 | 3 => {
                let err = fresh.undo_role().err().unwrap();
                assert_eq!(
                    (err.phase, err.effect),
                    (boot::Phase::Role, Effect::Unchanged)
                );
                assert!(matches!(err.cause, boot::Cause::UnsupportedPlan));
                assert!(!fresh.is_fenced());
            }
            _ => {
                let err = fresh.undo_role().err().unwrap();
                assert_eq!(
                    (err.phase, err.effect),
                    (boot::Phase::Role, Effect::CallerMustResolve)
                );
                let boot::Cause::Wal(wal) = err.cause else {
                    panic!("not WAL error")
                };
                assert_eq!(wal.effect, Effect::CallerMustResolve);
                assert!(match case {
                    4 => matches!(wal.cause, wal::Cause::Fenced),
                    _ => matches!(wal.cause, wal::Cause::Poisoned),
                });
                assert!(fresh.is_fenced());
            }
        }
        no_io(&state);
        mark_undo(&format!("UB03-{case:02}"));
    }
    for case in 6..=8 {
        let mut slot = port::slot();
        let mut workspace = boxed::<{ 4 << 20 }>(0xa5);
        let (installed, state) = if case == 8 {
            installed_seed(&mut slot, &mut workspace, true)
        } else {
            undo_installed(&mut slot, &mut workspace)
        };
        let storage = installed.into_mutable_storage();
        if case == 7 {
            poison(&storage.workspace);
        }
        match case {
            6 => undo_exact(storage.undo_role().unwrap(), storage.session_fence()),
            7 => {
                let err = storage.undo_role().err().unwrap();
                assert_eq!(err.effect, Effect::CallerMustResolve);
                assert!(matches!(err.cause, Cause::Poisoned));
                assert!(storage.session_fence().is_fenced());
            }
            _ => {
                let err = storage.undo_role().err().unwrap();
                assert_eq!(err.effect, Effect::Unchanged);
                let Cause::Bootstrap(boot) = err.cause else {
                    panic!("not bootstrap")
                };
                assert_eq!(boot.phase, boot::Phase::Role);
                assert!(matches!(boot.cause, boot::Cause::UnsupportedPlan));
                assert!(!storage.session_fence().is_fenced());
            }
        }
        no_io(&state);
        mark_undo(&format!("UB03-{case:02}"));
    }
}

fn assert_undo_partial(case: usize, state: &port::State) {
    let expected = std::fs::read_to_string(format!("{UNDO_INPUT}/partial-files.tsv")).unwrap();
    let id = format!("UB04-{case:02}");
    let mut count = 0;
    for line in expected.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        if fields[0] != id {
            continue;
        }
        let entry = &state.files[fields[1]];
        assert_eq!(entry.length, fields[2].parse::<u64>().unwrap());
        assert_eq!(entry.length, entry.bytes.len() as u64);
        assert!(entry.sparse_tail.is_empty());
        assert_eq!(super::native::hex(&content_digest(&entry.bytes)), fields[3]);
        count += 1;
    }
    assert_eq!(count, state.files.len());
    let events = std::fs::read_to_string(format!("{UNDO_INPUT}/effects.tsv")).unwrap();
    for (actual, expected) in state.events.iter().zip(events.lines()) {
        let fields: Vec<_> = expected.split('\t').collect();
        assert_eq!(
            actual.kind,
            if fields[0] == "directory_sync" {
                "dirsync"
            } else {
                fields[0]
            }
        );
        assert_eq!(actual.name, if fields[1] == "-" { "" } else { fields[1] });
        assert_eq!(actual.offset, fields[2].parse::<u64>().unwrap());
        assert_eq!(actual.length, fields[3].parse::<usize>().unwrap());
        assert_eq!(super::native::hex(&content_digest(&actual.data)), fields[4]);
    }
}

#[test]
fn undo_bootstrap_ub04_failed_creation() {
    for case in 1..=4 {
        let (writer, state) = port::fresh();
        match case {
            2 => {
                state
                    .lock()
                    .unwrap()
                    .read_edits
                    .push((11, "d3-bootstrap".into(), literal(1, true)))
            }
            3 => {
                state.lock().unwrap().read_plan = Some(port::Plan {
                    at: 11,
                    action: port::Action::BeforeError,
                })
            }
            4 => {
                state.lock().unwrap().plan = Some(port::Plan {
                    at: 13,
                    action: port::Action::AfterPanic,
                })
            }
            _ => (),
        }
        let mut slot = port::slot();
        let result = catch_unwind(AssertUnwindSafe(|| {
            boot::FreshStorageSession::create_undo(
                writer,
                if case == 1 { 0 } else { 1 },
                &mut slot,
                &mut [[0; PAGE_BYTES]; 3],
            )
            .map(drop)
        }));
        if case == 4 {
            assert!(result.is_err());
        } else {
            let err = result.unwrap().unwrap_err();
            assert_eq!(
                err.phase,
                if case == 1 {
                    boot::Phase::Preflight
                } else {
                    boot::Phase::Readback
                }
            );
            assert_eq!(
                err.effect,
                if case == 1 {
                    Effect::Unchanged
                } else {
                    Effect::CallerMustResolve
                }
            );
            match case {
                1 => assert!(matches!(err.cause, boot::Cause::InvalidInput)),
                2 => assert!(matches!(
                    err.cause,
                    boot::Cause::Control(ControlError::UnsupportedFormat)
                )),
                _ => {
                    let boot::Cause::Io(io) = err.cause else {
                        panic!("not IO")
                    };
                    assert_eq!(io.code, xadb_foundation::error::ErrorCode::IoRead);
                    assert_eq!(io.effect(), Effect::Unchanged);
                }
            }
        }
        let state = state.lock().unwrap();
        assert_eq!((state.handles, state.writers), (0, 0));
        assert_eq!(
            state.events.len(),
            match case {
                1 => 0,
                4 => 14,
                _ => 350,
            }
        );
        assert_eq!(state.hit, usize::from(case >= 3));
        assert_undo_partial(case, &state);
        if matches!(case, 2 | 3) {
            assert_eq!(state.reads, 12);
            assert_eq!(state.read_events[11].name, "d3-bootstrap");
            assert_eq!(state.continuation_read_hits, 0);
        }
        mark_undo(&format!("UB04-{case:02}"));
    }
}

#[test]
fn undo_bootstrap_ub05_old_consumers_refuse() {
    let mut slot = port::slot();
    let mut workspace = boxed::<{ 4 << 20 }>(0xa5);
    let (installed, state) = undo_installed(&mut slot, &mut workspace);
    let storage = installed.into_mutable_storage();
    let files = images(&state);
    let before = storage
        .workspace
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .as_slice()
        .to_vec();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut pages = [[0x3c; PAGE_BYTES]; 2];
    let [a, b] = &mut pages;
    let mut frames = [DirtyFrame::new(a), DirtyFrame::new(b)];
    let mut op = boxed::<114688>(0x5a);
    let mut allocation_op = boxed::<OPERATION_BYTES>(0x5a);
    let e = storage
        .bind_allocator(&core, &mut frames, &mut allocation_op)
        .err()
        .unwrap();
    assert_eq!(e.effect, Effect::Unchanged);
    assert!(matches!(e.cause, Cause::UnsupportedPlan));
    mark_undo("UB05-01");
    let e = storage
        .bind_status_bootstrap(&core, &mut frames, &mut op)
        .err()
        .unwrap();
    assert_eq!(e.effect, Effect::Unchanged);
    assert!(matches!(e.cause, Cause::UnsupportedPlan));
    mark_undo("UB05-02");
    let e = storage
        .bind_resident_status(&core, &mut frames, &mut op)
        .err()
        .unwrap();
    assert_eq!(e.effect, Effect::Unchanged);
    assert!(matches!(e.cause, Cause::UnsupportedPlan));
    assert_eq!(storage.binding.load(Ordering::Acquire), AVAILABLE);
    for frame in &mut frames {
        frame.check_empty().unwrap();
    }
    assert_eq!(pages, [[0x3c; PAGE_BYTES]; 2]);
    assert!(
        op.iter()
            .chain(allocation_op.iter())
            .all(|byte| *byte == 0x5a)
    );
    assert_eq!(
        storage
            .workspace
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .as_slice(),
        before
    );
    assert_eq!(images(&state), files);
    no_io(&state);
    mark_undo("UB05-03");
    drop(storage);

    let mut slot = port::slot();
    let mut workspace = boxed::<{ 4 << 20 }>(0xa5);
    let (installed, state) = undo_installed(&mut slot, &mut workspace);
    let files = images(&state);
    let before = installed._workspace.as_slice().to_vec();
    let mut page = [0x3c; PAGE_BYTES];
    let mut operation = boxed::<81920>(0x5a);
    let err = installed
        .into_allocation_session(&mut page, &mut operation)
        .err()
        .unwrap();
    assert_eq!(
        (err.phase, err.effect),
        (
            wal::AllocationSessionPhase::Constructor,
            Effect::CallerMustResolve
        )
    );
    assert!(matches!(
        err.cause,
        wal::AllocationSessionCause::UnsupportedPlan
    ));
    assert_eq!(page, [0x3c; PAGE_BYTES]);
    assert!(operation.iter().all(|byte| *byte == 0x5a));
    assert_eq!(workspace.as_slice(), before);
    assert_eq!(images(&state), files);
    no_io(&state);
    mark_undo("UB05-04");

    let mut scratch = [[0; PAGE_BYTES]; 3];
    let plan = undo_plan(1, &mut scratch);
    scratch[0][..4096].copy_from_slice(&undo_literal(1));
    let err = boot::inspect_initial_bootstrap(plan.profile, plan.selected, &mut scratch)
        .err()
        .unwrap();
    let boot::InitialRecoveryError::Bootstrap(err) = err else {
        panic!("not bootstrap")
    };
    assert!(matches!(err.cause, boot::Cause::UnsupportedPlan));
    mark_undo("UB05-05");
}

#[test]
#[ignore = "DEV3 S5 expanded plan3 descriptor matrix"]
fn undo_bootstrap_s5_plan_matrix() {
    let mut count = 0;
    for profile in 1..=3 {
        let plan = undo_plan(profile, &mut [[0; PAGE_BYTES]; 3]);
        for wire in [0, 1, 2, 3, 4, u64::MAX] {
            for high in [0, 1, 2, 3, u64::MAX] {
                if wire == 3 && high == 3
                    || (profile == 1 && ((wire != 3 && high == 3) || (wire == 3 && high != 3)))
                {
                    continue;
                }
                let raw = changed(changed(undo_literal(profile), 280, wire), 120, high);
                assert_eq!(
                    plan.decode_descriptor(&raw),
                    if wire != 3 {
                        Err(ControlError::UnsupportedFormat)
                    } else {
                        Err(ControlError::Corrupt)
                    }
                );
                mark_undo(&format!("UB-S5-P{profile}-W{wire}-H{high}"));
                count += 1;
            }
        }
    }
    assert_eq!(count, 78);
}
