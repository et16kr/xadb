use super::*;
use crate::transaction::concurrency::PageConcurrency;
use std::sync::atomic::AtomicU8;

fn with_ledger(f: impl FnOnce(&Mutex<IsolationLedger<'_, '_, '_>>, &SessionFence)) {
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let binding = AtomicU8::new(super::super::super::super::BOUND);
    let mut bytes = [0; ISOLATION_BYTES];
    let ledger = Mutex::new(IsolationLedger {
        bytes: &mut bytes,
        core: &core,
        binding: &binding,
        fence: &fence,
    });
    f(&ledger, &fence);
}

fn used(ledger: &Mutex<IsolationLedger<'_, '_, '_>>) -> u64 {
    let guard = ledger.lock().unwrap_or_else(|poison| poison.into_inner());
    u64::from_le_bytes(guard.bytes[USED].try_into().unwrap())
}

#[test]
fn exact_aggregate_capacity_releases_only_its_own_charge() {
    with_ledger(|ledger, fence| {
        ledger.lock().unwrap().bytes[16..24].copy_from_slice(&512u64.to_le_bytes());
        let mut first = OperationSourceAdmission::reserve(ledger, 32768, 16384).unwrap();
        let second = OperationSourceAdmission::reserve(ledger, 32768, 16384).unwrap();
        assert_eq!(used(ledger), 32768);
        let before = *ledger.lock().unwrap().bytes;
        let error = OperationSourceAdmission::reserve(ledger, 32768, 1)
            .err()
            .unwrap();
        assert!(matches!(error.cause, SourceCause::Capacity));
        assert_eq!(error.effect, Effect::Unchanged);
        assert_eq!(*ledger.lock().unwrap().bytes, before);
        first.release_checked().unwrap();
        first.release_checked().unwrap();
        assert_eq!(used(ledger), 16384);
        let replacement = OperationSourceAdmission::reserve(ledger, 32768, 16384).unwrap();
        drop(second);
        assert_eq!(used(ledger), 16384);
        drop(replacement);
        assert_eq!(used(ledger), 0);
        assert_eq!(&ledger.lock().unwrap().bytes[16..24], &512u64.to_le_bytes());
        assert!(!fence.is_fenced());
    });
}

#[test]
fn simultaneous_reservations_cannot_overbook_the_original_ledger() {
    with_ledger(|ledger, fence| {
        let barrier = std::sync::Barrier::new(3);
        std::thread::scope(|scope| {
            let run = || {
                barrier.wait();
                let ticket = OperationSourceAdmission::reserve(ledger, 32768, 32768);
                barrier.wait();
                let admitted = ticket.is_ok();
                if let Err(error) = &ticket {
                    assert!(matches!(error.cause, SourceCause::Capacity));
                    assert_eq!(error.effect, Effect::Unchanged);
                }
                barrier.wait();
                drop(ticket);
                admitted
            };
            let first = scope.spawn(run);
            let second = scope.spawn(run);
            barrier.wait();
            barrier.wait();
            assert_eq!(used(ledger), 32768);
            barrier.wait();
            assert_ne!(first.join().unwrap(), second.join().unwrap());
        });
        assert_eq!(used(ledger), 0);
        assert!(!fence.is_fenced());
    });
}

#[test]
fn overflow_and_invalid_header_do_not_create_capacity() {
    with_ledger(|ledger, fence| {
        let error = OperationSourceAdmission::reserve(ledger, 1, usize::MAX)
            .err()
            .unwrap();
        assert!(matches!(error.cause, SourceCause::Capacity));
        assert_eq!(used(ledger), 0);
        assert!(!fence.is_fenced());
        ledger.lock().unwrap().bytes[32] = 1;
        let error = OperationSourceAdmission::reserve(ledger, 1, 1)
            .err()
            .unwrap();
        assert!(matches!(error.cause, SourceCause::Invariant));
        assert_eq!(error.effect, Effect::CallerMustResolve);
        assert_eq!(used(ledger), 0);
        assert!(fence.is_fenced());
    });
}

#[test]
fn underflow_fences_without_returning_the_charge() {
    with_ledger(|ledger, fence| {
        let mut ticket = OperationSourceAdmission::reserve(ledger, 32768, 4096).unwrap();
        ledger.lock().unwrap().bytes[USED].fill(0);
        let error = ticket.release_checked().unwrap_err();
        assert!(matches!(error.cause, SourceCause::Invariant));
        assert!(ticket.active);
        assert!(fence.is_fenced());
        drop(ticket);
        assert_eq!(used(ledger), 0);
    });
}

#[test]
fn poison_and_fence_preserve_the_outstanding_charge() {
    with_ledger(|ledger, fence| {
        let mut ticket = OperationSourceAdmission::reserve(ledger, 32768, 4096).unwrap();
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = ledger.lock().unwrap();
            panic!("poison the original isolation mutex");
        }));
        assert!(panic.is_err());
        let error = ticket.release_checked().unwrap_err();
        assert!(matches!(error.cause, SourceCause::Poisoned));
        assert!(fence.is_fenced());
        assert!(ledger.is_poisoned());
        assert_eq!(used(ledger), 4096);
        drop(ticket);
        assert_eq!(used(ledger), 4096);
    });
    with_ledger(|ledger, fence| {
        let mut ticket = OperationSourceAdmission::reserve(ledger, 32768, 4096).unwrap();
        fence.fence();
        assert!(matches!(
            ticket.release_checked().unwrap_err().cause,
            SourceCause::Fenced
        ));
        assert_eq!(used(ledger), 4096);
    });
}

#[test]
fn unwind_and_forgotten_loans_never_manufacture_capacity() {
    with_ledger(|ledger, fence| {
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ticket = OperationSourceAdmission::reserve(ledger, 32768, 4096).unwrap();
            panic!("unwind while the source loan is live");
        }));
        assert!(panic.is_err());
        assert!(fence.is_fenced());
        assert_eq!(used(ledger), 4096);
    });
    with_ledger(|ledger, fence| {
        let ticket = OperationSourceAdmission::reserve(ledger, 32768, 32768).unwrap();
        std::mem::forget(ticket);
        assert!(matches!(
            OperationSourceAdmission::reserve(ledger, 32768, 1)
                .err()
                .unwrap()
                .cause,
            SourceCause::Capacity
        ));
        assert_eq!(used(ledger), 32768);
        assert!(!fence.is_fenced());
    });
}

#[test]
fn catalog_class_uses_current_types_and_recovered_retained_owner() {
    type W = crate::storage::wal_io::plan4_test_support::Writer;
    let live = super::super::resources::source_census::<W>();
    let recovered = super::super::super::super::plan4_recovery::source_census::<W>();
    let shared = shared_bytes::<W>();
    assert_eq!(
        shared,
        live.retained.max(recovered.retained_live) + split_port_bytes::<W>()
    );
    assert!(shared + live.catalog_lookup <= 65536);
    println!(
        "CATALOG_SHORT shared={shared} charge={} total={} leaf={}",
        live.catalog_lookup,
        shared + live.catalog_lookup,
        leaf_source_bytes()
    );
    with_ledger(|ledger, fence| {
        let mut first = OperationSourceAdmission::catalog(
            ledger,
            &CatalogSourceCharges::new::<W>(),
            OperationClass::CatalogFull,
        )
        .unwrap();
        assert_eq!(used(ledger) as usize, live.catalog_lookup);
        assert!(matches!(
            OperationSourceAdmission::catalog(
                ledger,
                &CatalogSourceCharges::new::<W>(),
                OperationClass::CatalogFull
            )
            .err()
            .unwrap()
            .cause,
            SourceCause::Capacity
        ));
        first.release_checked().unwrap();
        assert_eq!(used(ledger), 0);
        assert!(!fence.is_fenced());
    });
}

#[test]
fn fixed_resident_class_admits_two_and_never_escalates() {
    type W = crate::storage::wal_io::plan4_test_support::Writer;
    let shared = shared_bytes::<W>();
    let resident = read::resident_lookup_source_bytes::<W>();
    let full = read::lookup_source_bytes::<W>();
    println!(
        "CATALOG_RESIDENT shared={shared} resident={resident} full={full} two={}",
        shared + resident * 2
    );
    assert!(resident < full);
    assert!(shared + resident * 2 <= 65536);
    assert!(shared + resident + full > 65536);
    with_ledger(|ledger, fence| {
        let mut first = OperationSourceAdmission::catalog(
            ledger,
            &CatalogSourceCharges::new::<W>(),
            OperationClass::CatalogResident,
        )
        .unwrap();
        assert!(first.resident());
        let mut second = OperationSourceAdmission::catalog(
            ledger,
            &CatalogSourceCharges::new::<W>(),
            OperationClass::CatalogResident,
        )
        .unwrap();
        assert_eq!(used(ledger), 2 * resident as u64);
        let before = *ledger.lock().unwrap().bytes;
        assert!(matches!(
            OperationSourceAdmission::catalog(
                ledger,
                &CatalogSourceCharges::new::<W>(),
                OperationClass::CatalogFull
            )
            .err()
            .unwrap()
            .cause,
            SourceCause::Capacity
        ));
        assert_eq!(*ledger.lock().unwrap().bytes, before);
        first.release_checked().unwrap();
        assert_eq!(used(ledger), resident as u64);
        assert!(second.resident());
        second.release_checked().unwrap();
        let full_ticket = OperationSourceAdmission::catalog(
            ledger,
            &CatalogSourceCharges::new::<W>(),
            OperationClass::CatalogFull,
        )
        .unwrap();
        assert!(!full_ticket.resident());
        assert_eq!(used(ledger), full as u64);
        drop(full_ticket);
        assert_eq!(used(ledger), 0);
        assert!(!fence.is_fenced());
    });
}

#[test]
fn ticket_width_preserves_full_65536_charge_and_rejects_overflow() {
    with_ledger(|ledger, fence| {
        let ticket = OperationSourceAdmission::reserve(ledger, 0, 65_536).unwrap();
        assert_eq!(ticket.charge, 65_536);
        assert_eq!(used(ledger), 65_536);
        drop(ticket);
        assert_eq!(used(ledger), 0);
        for charge in [65_537, u32::MAX as usize + 1, usize::MAX] {
            let error = OperationSourceAdmission::reserve(ledger, 0, charge)
                .err()
                .unwrap();
            assert!(matches!(error.cause, SourceCause::Capacity));
            assert_eq!(error.effect, Effect::Unchanged);
            assert_eq!(used(ledger), 0);
        }
        assert_eq!(size_of::<OperationSourceAdmission<'_, '_, '_, '_>>(), 16);
        assert!(!fence.is_fenced());
    });
}

#[test]
fn catalog_native_class_includes_core_once_and_fits() {
    type W = super::super::super::native::Writer;
    let live = super::super::resources::source_census::<W>();
    let recovered = super::super::super::super::plan4_recovery::source_census::<W>();
    assert_eq!(
        shared_bytes::<W>(),
        live.retained.max(recovered.retained_live) + split_port_bytes::<W>()
    );
    assert!(super::super::resources::fits::<W>());
    println!(
        "CATALOG_SHORT_NATIVE shared={} charge={} total={}",
        shared_bytes::<W>(),
        live.catalog_lookup,
        shared_bytes::<W>() + live.catalog_lookup
    );
}

#[test]
fn stale_binding_rejects_reservation_and_preserves_outstanding_charge() {
    with_ledger(|ledger, fence| {
        let mut ticket = OperationSourceAdmission::reserve(ledger, 32768, 4096).unwrap();
        ledger.lock().unwrap().binding.store(0, Ordering::Release);
        let error = OperationSourceAdmission::reserve(ledger, 32768, 1)
            .err()
            .unwrap();
        assert!(matches!(error.cause, SourceCause::BindingMismatch));
        assert_eq!(error.effect, Effect::CallerMustResolve);
        assert_eq!(used(ledger), 4096);
        assert!(ticket.release_checked().is_err());
        assert!(fence.is_fenced());
        assert_eq!(used(ledger), 4096);
    });
}

fn commit_charges() -> CatalogCommitCharges {
    CatalogCommitCharges {
        shared: 32768,
        stage: 16384,
        pending: 4096,
        install: 12288,
        publish: 8192,
    }
}

#[test]
fn retirement_fixed_charge_fits_and_cannot_use_commit_transitions() {
    fn inspect<W: WriterNamespace>(kind: &str) {
        let shared = shared_bytes::<W>();
        let charge = CatalogProgressCharge::new::<W>();
        let phases = create_pair::catalog_live::catalog_live_source_bytes::<W>();
        assert!(charge.0 >= phases.lifecycle);
        assert!(charge.0 >= phases.prepare + retirement_prepare_carrier_bytes());
        assert!(charge.0 >= retirement_source_peak());
        let live = super::super::resources::source_census::<W>();
        println!(
            "RETIREMENT_SOURCE_{kind} shared={shared} charge={} total={} phases={phases:?} live={live:?}",
            charge.0,
            shared + charge.0
        );
        assert!(shared + charge.0 <= 65536);
        assert!(live.catalog_live >= charge.0);
        assert!(super::super::resources::fits::<W>());
    }
    inspect::<crate::storage::wal_io::plan4_test_support::Writer>("MEMORY");
    inspect::<super::super::super::native::Writer>("NATIVE_TYPE");
    with_ledger(|ledger, fence| {
        let core = ledger.lock().unwrap().core;
        let mut owner = core
            .try_owner(crate::transaction::concurrency::OwnerRole::Maintenance)
            .unwrap();
        let gate = owner.close_reads_for_retirement(core).unwrap();
        let charge = CatalogProgressCharge(8192);
        let mut ticket =
            OperationSourceAdmission::catalog_progress(ledger, &commit_charges(), &charge, &gate)
                .unwrap();
        assert_eq!(used(ledger), 8192);
        let error = ticket
            .enter_commit_pending(&commit_charges())
            .err()
            .unwrap();
        assert!(matches!(error.cause, SourceCause::Invariant));
        assert!(fence.is_fenced());
        assert!(ticket.finish_checked().is_err());
        assert_eq!(used(ledger), 8192);
        assert!(gate.reopen_checked().is_err());
    });
}

#[test]
fn commit_class_moves_only_at_gate_and_drain_boundaries() {
    use crate::transaction::concurrency::OwnerRole;
    with_ledger(|ledger, fence| {
        let core = ledger.lock().unwrap().core;
        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
        let gate = owner.close_reads_for_terminal(core).unwrap();
        let charges = commit_charges();
        let mut writer = OperationSourceAdmission::catalog_commit(ledger, &charges, &gate).unwrap();
        assert_eq!(used(ledger), 16384);
        writer.enter_commit_pending(&charges).unwrap();
        assert_eq!(used(ledger), 4096);
        // A reader admitted during Pending shares the one aggregate.
        let reader = OperationSourceAdmission::reserve(ledger, 32768, 16384).unwrap();
        let error = OperationSourceAdmission::reserve(ledger, 32768, 12289)
            .err()
            .unwrap();
        assert!(matches!(error.cause, SourceCause::Capacity));
        assert_eq!(used(ledger), 20480);
        drop(reader);
        // The Drain boundary grows in place without a second reservation.
        writer
            .transition(charges.shared, charges.pending, charges.install, true)
            .unwrap();
        assert_eq!(used(ledger), 12288);
        writer.enter_commit_publish(&charges).unwrap();
        assert_eq!(used(ledger), 8192);
        writer.finish_checked().unwrap();
        assert_eq!(used(ledger), 0);
        gate.reopen_checked().unwrap();
        owner.check_idle().unwrap();
        assert!(!fence.is_fenced());
    });
}

#[test]
fn commit_growth_beside_a_held_loan_fences_and_keeps_charges() {
    use crate::transaction::concurrency::OwnerRole;
    with_ledger(|ledger, fence| {
        let core = ledger.lock().unwrap().core;
        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
        let gate = owner.close_reads_for_terminal(core).unwrap();
        let charges = commit_charges();
        let mut writer = OperationSourceAdmission::catalog_commit(ledger, &charges, &gate).unwrap();
        writer.enter_commit_pending(&charges).unwrap();
        let _reader = OperationSourceAdmission::reserve(ledger, 32768, 24576).unwrap();
        // Only a broken Drain could leave this loan; growth never waits.
        let error = writer
            .transition(charges.shared, charges.pending, charges.install, true)
            .err()
            .unwrap();
        assert!(matches!(error.cause, SourceCause::Invariant));
        assert_eq!(error.effect, Effect::CallerMustResolve);
        assert!(fence.is_fenced());
        assert_eq!(used(ledger), 28672);
        assert!(writer.release_checked().is_err());
        assert_eq!(used(ledger), 28672);
    });
}

#[test]
fn commit_boundaries_reject_wrong_order_and_reader_entry() {
    use crate::transaction::concurrency::OwnerRole;
    type W = crate::storage::wal_io::plan4_test_support::Writer;
    // A reversed window order is refused before any effect.
    with_ledger(|ledger, fence| {
        let core = ledger.lock().unwrap().core;
        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
        let gate = owner.close_reads_for_terminal(core).unwrap();
        let charges = CatalogCommitCharges {
            pending: 20000,
            ..commit_charges()
        };
        let error = OperationSourceAdmission::catalog_commit(ledger, &charges, &gate)
            .err()
            .unwrap();
        assert!(matches!(error.cause, SourceCause::Capacity));
        assert_eq!(error.effect, Effect::Unchanged);
        assert!(!fence.is_fenced());
        assert_eq!(used(ledger), 0);
    });
    // Pending itself can only shrink.
    with_ledger(|ledger, fence| {
        let core = ledger.lock().unwrap().core;
        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
        let gate = owner.close_reads_for_terminal(core).unwrap();
        let charges = commit_charges();
        let mut writer = OperationSourceAdmission::catalog_commit(ledger, &charges, &gate).unwrap();
        let error = writer
            .transition(charges.shared, charges.stage, charges.stage + 1, false)
            .err()
            .unwrap();
        assert!(matches!(error.cause, SourceCause::Invariant));
        assert!(fence.is_fenced());
        assert_eq!(used(ledger), 16384);
    });
    // A gate closed on another core is not proof for this ledger.
    with_ledger(|ledger, fence| {
        let other = PageConcurrency::new(fence).unwrap();
        let mut owner = other.try_owner(OwnerRole::Foreground).unwrap();
        let gate = owner.close_reads_for_terminal(&other).unwrap();
        let error = OperationSourceAdmission::catalog_commit(ledger, &commit_charges(), &gate)
            .err()
            .unwrap();
        assert!(matches!(error.cause, SourceCause::Invariant));
        assert!(fence.is_fenced());
        assert_eq!(used(ledger), 0);
    });
    // A repeated boundary does not match the current window.
    with_ledger(|ledger, fence| {
        let core = ledger.lock().unwrap().core;
        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
        let gate = owner.close_reads_for_terminal(core).unwrap();
        let charges = commit_charges();
        let mut writer = OperationSourceAdmission::catalog_commit(ledger, &charges, &gate).unwrap();
        writer.enter_commit_pending(&charges).unwrap();
        let error = writer.enter_commit_pending(&charges).err().unwrap();
        assert!(matches!(error.cause, SourceCause::Invariant));
        assert!(fence.is_fenced());
        assert_eq!(used(ledger), 4096);
    });
    // A reader entry cannot select the writer class.
    with_ledger(|ledger, fence| {
        let error = OperationSourceAdmission::catalog(
            ledger,
            &CatalogSourceCharges::new::<W>(),
            OperationClass::CommitProgress,
        )
        .err()
        .unwrap();
        assert!(matches!(error.cause, SourceCause::Invariant));
        assert!(fence.is_fenced());
        assert_eq!(used(ledger), 0);
    });
}

#[test]
fn catalog_commit_windows_fit_beside_admitted_readers() {
    fn inspect<W: WriterNamespace>(kind: &str) {
        let [reader_shared, full, resident] = CatalogSourceCharges::new::<W>().observations();
        let [shared, stage, pending, install, publish] =
            CatalogCommitCharges::new::<W>().observations();
        let [raw_stage, raw_pending, raw_install, raw_publish] =
            super::super::create_pair::catalog_commit_writer_windows();
        let parts = super::super::create_pair::catalog_commit_pending_parts();
        let physical = crate::storage::dirty_frame::commit_source_phases::<4>();
        let wait = super::super::terminal::catalog_wait_source_bytes();
        let measured_wait = super::super::terminal::catalog_wait_source_observation::<W>();
        println!(
            "COMMIT_PENDING_PARTS_{kind} parent_wait_drain={parts:?} physical_pending={} physical_drain={} bridge={} wait={wait} measured_wait={measured_wait} drain_call={} pin_release={}",
            physical.pending,
            physical.drain,
            super::super::terminal::catalog_source_bytes(),
            crate::storage::dirty_frame::commit_drain_call_source_bytes::<4>(),
            crate::storage::dirty_frame::pin_release_source_bytes()
        );
        // The production wait bound covers the measured wait adapter closure.
        assert!(wait >= measured_wait);
        assert_eq!(commit_source_peak(), stage.max(install));
        println!(
            "COMMIT_WINDOWS_{kind} shared={shared} stage={stage} pending={pending} install={install} publish={publish} raw={raw_stage}/{raw_pending}/{raw_install}/{raw_publish} carriers={} full={full} resident={resident} stage_total={} install_total={} pending_full={} publish_full={} pending_two_resident={}",
            stage - raw_stage,
            shared + stage,
            shared + install,
            shared + pending + full,
            shared + publish + full,
            shared + pending + 2 * resident
        );
        assert_eq!(shared, reader_shared);
        assert!(pending < stage && pending < install && publish < install);
        // Reads are closed for stage and drained for install; Pending and
        // publication overlap one admitted Full read.
        assert!(shared + stage <= 65536 && shared + install <= 65536);
        assert!(shared + pending + full <= 65536);
        assert!(shared + publish + full <= 65536);
    }
    inspect::<crate::storage::wal_io::plan4_test_support::Writer>("MEMORY");
    inspect::<super::super::super::native::Writer>("NATIVE_TYPE");
}
