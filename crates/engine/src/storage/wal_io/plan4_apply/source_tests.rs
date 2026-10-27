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
        assert!(matches!(error.cause, RetentionCause::Capacity));
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
                    assert!(matches!(error.cause, RetentionCause::Capacity));
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
        assert!(matches!(error.cause, RetentionCause::Capacity));
        assert_eq!(used(ledger), 0);
        assert!(!fence.is_fenced());
        ledger.lock().unwrap().bytes[32] = 1;
        let error = OperationSourceAdmission::reserve(ledger, 1, 1)
            .err()
            .unwrap();
        assert!(matches!(error.cause, RetentionCause::Invariant));
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
        assert!(matches!(error.cause, RetentionCause::Invariant));
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
        assert!(matches!(error.cause, RetentionCause::Poisoned));
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
            RetentionCause::Fenced
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
            RetentionCause::Capacity
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
    assert_eq!(shared, live.retained.max(recovered.retained_live));
    assert!(shared + live.catalog_lookup <= 65536);
    println!(
        "CATALOG_SHORT shared={shared} charge={} total={} leaf={}",
        live.catalog_lookup,
        shared + live.catalog_lookup,
        leaf_source_bytes()
    );
    with_ledger(|ledger, fence| {
        let mut first =
            OperationSourceAdmission::catalog::<W>(ledger, OperationClass::CatalogFull).unwrap();
        assert_eq!(used(ledger) as usize, live.catalog_lookup);
        assert!(matches!(
            OperationSourceAdmission::catalog::<W>(ledger, OperationClass::CatalogFull)
                .err()
                .unwrap()
                .cause,
            RetentionCause::Capacity
        ));
        first.release_checked().unwrap();
        assert_eq!(used(ledger), 0);
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
        live.retained.max(recovered.retained_live)
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
        assert!(matches!(error.cause, RetentionCause::BindingMismatch));
        assert_eq!(error.effect, Effect::CallerMustResolve);
        assert_eq!(used(ledger), 4096);
        assert!(ticket.release_checked().is_err());
        assert!(fence.is_fenced());
        assert_eq!(used(ledger), 4096);
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
    type Native = super::super::super::native::Writer;
    let native_shared = shared_bytes::<Native>();
    let native_resident = read::resident_lookup_source_bytes::<Native>();
    println!(
        "CATALOG_RESIDENT_NATIVE shared={native_shared} resident={native_resident} two={}",
        native_shared + 2 * native_resident
    );
    assert!(native_shared + 2 * native_resident <= 65536);
    assert!(resident < full);
    assert!(shared + resident * 2 <= 65536);
    assert!(shared + resident + full > 65536);
    with_ledger(|ledger, fence| {
        let mut first =
            OperationSourceAdmission::catalog::<W>(ledger, OperationClass::CatalogResident)
                .unwrap();
        assert!(first.resident());
        let mut second =
            OperationSourceAdmission::catalog::<W>(ledger, OperationClass::CatalogResident)
                .unwrap();
        assert_eq!(used(ledger), 2 * resident as u64);
        let before = *ledger.lock().unwrap().bytes;
        assert!(matches!(
            OperationSourceAdmission::catalog::<W>(ledger, OperationClass::CatalogFull)
                .err()
                .unwrap()
                .cause,
            RetentionCause::Capacity
        ));
        assert_eq!(*ledger.lock().unwrap().bytes, before);
        first.release_checked().unwrap();
        assert_eq!(used(ledger), resident as u64);
        assert!(second.resident());
        second.release_checked().unwrap();
        let full_ticket =
            OperationSourceAdmission::catalog::<W>(ledger, OperationClass::CatalogFull).unwrap();
        assert!(!full_ticket.resident());
        assert_eq!(used(ledger), full as u64);
        drop(full_ticket);
        assert_eq!(used(ledger), 0);
        assert!(!fence.is_fenced());
    });
}
