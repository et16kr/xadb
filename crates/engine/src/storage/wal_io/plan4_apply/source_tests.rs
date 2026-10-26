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
        let mut first = OperationSourceAdmission::catalog::<W>(ledger).unwrap();
        assert_eq!(used(ledger) as usize, live.catalog_lookup);
        assert!(matches!(
            OperationSourceAdmission::catalog::<W>(ledger)
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
