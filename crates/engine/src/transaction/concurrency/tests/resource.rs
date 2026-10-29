//! Target-layout observations for the separately reviewed logical value ledger.
//! This does not measure compiler frames, OS stacks or total session admission.

use super::*;
use std::iter::Enumerate;
use std::slice::Iter;
use std::sync::{LockResult, TryLockResult};

pub(super) fn type_ceilings() {
    let layouts = [
        ("core", size_of::<PageConcurrency<'_>>(), 2048),
        ("owner", size_of::<Owner<'_, '_>>(), 64),
        ("apply", size_of::<ApplyAdmission<'_, '_, '_>>(), 80),
        ("content", size_of::<ContentAdmission<'_, '_, '_>>(), 80),
        ("version", size_of::<VersionEditGuard<'_, '_, '_>>(), 384),
        (
            "simulated_stage",
            size_of::<support::SimulatedCommitStage<'_, '_, '_>>(),
            384,
        ),
        (
            "simulated_pending",
            size_of::<support::SimulatedInstallation<'_, '_, '_>>(),
            384,
        ),
        (
            "simulated_install",
            size_of::<support::SimulatedInstallGuard<'_, '_, '_>>(),
            384,
        ),
        ("slot", size_of::<OwnerSlot>(), 16),
        ("slots", size_of::<[OwnerSlot; 34]>(), 544),
        ("state", size_of::<State>(), 1024),
        ("state_mutex", size_of::<Mutex<State>>(), 1040),
        (
            "core_result",
            size_of::<Result<PageConcurrency<'_>>>(),
            2056,
        ),
        ("state_guard", size_of::<MutexGuard<'_, State>>(), 16),
        (
            "lock_result",
            size_of::<LockResult<MutexGuard<'_, State>>>(),
            24,
        ),
        (
            "try_lock_result",
            size_of::<TryLockResult<MutexGuard<'_, ()>>>(),
            24,
        ),
        (
            "cleanup_pair",
            size_of::<(MutexGuard<'_, State>, Option<ConcurrencyError>)>(),
            24,
        ),
        ("small_result", size_of::<Result<()>>(), 8),
        ("small_error", size_of::<ConcurrencyError>(), 8),
        ("optional_error", size_of::<Option<ConcurrencyError>>(), 8),
        ("iterator", size_of::<Enumerate<Iter<'_, OwnerSlot>>>(), 24),
        (
            "iterator_item",
            size_of::<Option<(usize, &OwnerSlot)>>(),
            24,
        ),
        ("reference", size_of::<&OwnerSlot>(), 8),
        ("optional_reference", size_of::<Option<&OwnerSlot>>(), 8),
        ("usize", size_of::<usize>(), 8),
        ("oracle", size_of::<Model>(), 1024),
        ("observation", size_of::<support::Observation>(), 1024),
    ];
    for (name, actual, ceiling) in layouts {
        assert!(actual <= ceiling, "{name}: {actual} exceeds {ceiling}");
        println!("GUARD_LAYOUT {name} actual={actual} ceiling={ceiling}");
    }
    let retained = 2048 + 34 * 64 + 33 * 80 + 384;
    let temporary = 384 + 4 * 128;
    let constructor = 544 + 2 * 1024 + 1040 + 2048 + 2056 + 96;
    assert_eq!(retained, 7248);
    assert_eq!(temporary, 896);
    assert_eq!(constructor, 7832);
    println!(
        "GUARD_LOGICAL_LEDGER retained={retained} nested_temporary={temporary} constructor={constructor} global_callers=UNPROVEN physical_stack=NOT_MEASURED"
    );
}

pub(super) fn all_live_control() {
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let mut model = Model::new();
    let mut owners: [Owner<'_, '_>; 34] = std::array::from_fn(|index| {
        let role = match index {
            0..=31 => OwnerRole::Foreground,
            32 => OwnerRole::Maintenance,
            33 => OwnerRole::DedicatedFlusher,
            _ => unreachable!(),
        };
        issue(&core, &mut model, role)
    });
    let (first, remaining) = owners.split_first_mut().unwrap();
    let version = first.try_apply().unwrap().try_version().unwrap();
    model.modes[0] = APPLY;
    model.check(&core);
    let mut iterator = remaining.iter_mut();
    let contents: [ContentAdmission<'_, '_, '_>; 33] = std::array::from_fn(|index| {
        let content = iterator.next().unwrap().try_content().unwrap();
        model.modes[index + 1] = CONTENT;
        model.check(&core);
        content
    });
    assert!(iterator.next().is_none());
    assert!(support::version_busy(&core));
    let observed = support::observe(&core);
    assert_eq!((observed.active_apply, observed.active_content), (1, 33));
    drop(contents);
    model.modes[1..].fill(IDLE);
    model.check(&core);
    drop(version);
    model.modes[0] = IDLE;
    model.check(&core);
    assert!(!support::version_busy(&core));
    for (index, owner) in owners.into_iter().enumerate() {
        drop(owner);
        model.release(index);
        model.check(&core);
    }
    assert_eq!(
        (
            support::observe(&core).active_apply,
            support::observe(&core).active_content
        ),
        (0, 0)
    );
    assert!(!fence.is_fenced());
}
