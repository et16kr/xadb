//! Tests of the actual consuming handoff; no transaction/WAL effect is claimed.
use super::*;
use crate::{
    storage::{
        dirty_frame::{DirtyFrame, init_test_support as probe},
        page::PAGE_BYTES,
        wal_io::{
            self as wal, FreshStorageSession, IntoFlushError, bootstrap::test_support as port,
        },
    },
    transaction::concurrency::{OwnerRole, PageConcurrency},
};
use std::panic::{AssertUnwindSafe, catch_unwind};

fn boxed<const N: usize>(byte: u8) -> Box<[u8; N]> {
    vec![byte; N].into_boxed_slice().try_into().unwrap()
}
macro_rules! fixture {
    (|$bound:ident, $core:ident, $state:ident, $storage:ident, $bank:ident $(, $foreign:ident)?| $body:block) => {{
        let mut wal_slot = port::slot();
        let mut workspace = boxed::<{ 4 << 20 }>(0);
        let (namespace, $state) = port::fresh();
        let fresh = FreshStorageSession::create_status(
            namespace,
            1,
            &mut wal_slot,
            &mut [[0; PAGE_BYTES]; 3],
        )
        .unwrap();
        let installed = fresh.install_doublewrite(&mut workspace).unwrap();
        let $storage = installed.into_mutable_storage();
        let $core = PageConcurrency::new($storage.session_fence()).unwrap();
        $(let $foreign = PageConcurrency::new($storage.session_fence()).unwrap();)?
        let mut a = boxed::<PAGE_BYTES>(0xa1);
        let mut b = boxed::<PAGE_BYTES>(0xb2);
        let mut c = boxed::<PAGE_BYTES>(0xc3);
        let mut operation = boxed::<114688>(0x73);
        let mut frames = [
            DirtyFrame::new(&mut a),
            DirtyFrame::new(&mut b),
            DirtyFrame::new(&mut c),
        ];
        let binding = $storage
            .bind_resident_status(&$core, &mut frames, &mut operation)
            .unwrap();
        let mut $bound = match binding.activate() {
            Ok(value) => value,
            Err(IntoFlushError::Before { error, .. }) | Err(IntoFlushError::Fatal(error)) => {
                panic!("activate {error:?}")
            }
        };
        let mut $bank = boxed::<EMPTY_TRANSACTION_BANK_BYTES>(0x6d);
        $body
    }};
}
macro_rules! init {
    ($bound:ident, $owner:ident) => {{
        let (mut writer, _, _) = $bound.split_with_flush();
        match writer.reserve_one(&mut $owner).unwrap().initialize() {
            Ok(value) => value,
            Err(wal::InitAttemptError::Before(e)) => panic!("INIT before {:?}", e.error),
            Err(wal::InitAttemptError::Fatal(e)) => panic!("INIT fatal {e:?}"),
        }
    }};
}
fn file_bytes(state: &port::State) -> Vec<(String, Vec<u8>)> {
    state
        .files
        .iter()
        .map(|(name, file)| (name.clone(), file.bytes.clone()))
        .collect()
}

#[test]
fn empty_handoff_resource_gate() {
    type W = port::Writer;
    println!(
        "conversion={} retained={} metadata={} with_flush={}",
        conversion_bytes::<W>(),
        retained_bytes::<W>(),
        retained_bytes::<W>() + conversion_bytes::<W>(),
        retained_bytes::<W>() + conversion_bytes::<W>() + 65536
    );
    assert!(handoff_fits::<W>());
    assert!(handoff_fits::<xadb_platform::namespace::LinuxWriterLease>());
    assert_eq!(EMPTY_TRANSACTION_BANK_BYTES, 53248);
    assert!(
        size_of::<EmptyTransactions<'_, '_, '_, '_, '_, '_, '_, '_, W>>()
            - size_of::<ResidentStatus<'_, '_, '_, '_, '_, '_, '_, W>>()
            <= 64 + size_of::<Option<crate::storage::wal_io::WalRequirement<'_>>>()
    );
}

#[test]
fn empty_handoff_dirty_system_and_two_status_pages_preserve_identity() {
    fixture!(|bound, core, state, storage, bank| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let a = init!(bound, owner);
        bound.prepare_slot(2).unwrap();
        let b = init!(bound, owner);
        assert_eq!((a.lsn.0, b.lsn.0), (2, 4));
        let descriptor = bound.inner.inner.control.base.descriptor;
        let mut bytes = boxed::<PAGE_BYTES>(0);
        let before = probe::snapshot(&bound.inner.inner.pool, 0, &mut bytes);
        let before_bytes = bytes.clone();
        let files = file_bytes(&state.lock().unwrap());
        let prefix = storage.wal().prefix().unwrap();
        let value = match bound.into_empty_transactions(&mut owner, &mut bank) {
            Ok(value) => value,
            Err(_) => panic!("healthy handoff rejected"),
        };
        assert_eq!(
            value.snapshot().unwrap(),
            EmptyTransactionSnapshot {
                frozen_system_lsn: PageLsn(4),
                issued_local: 0,
                adopted_local: 0,
                retained_bank_bytes: 53248,
            }
        );
        assert_eq!(value.inner.inner.inner.control.base.descriptor, descriptor);
        assert_eq!(
            probe::snapshot(&value.inner.inner.inner.pool, 0, &mut bytes),
            before
        );
        assert_eq!(bytes, before_bytes);
        assert!(value.bank.iter().all(|byte| *byte == 0));
        assert_eq!(storage.wal().prefix().unwrap(), prefix);
        assert_eq!(file_bytes(&state.lock().unwrap()), files);
        assert!(!storage.session_fence().is_fenced());
        drop(value);
        assert!(storage.session_fence().is_fenced());
    });
}

#[test]
fn empty_handoff_without_status_returns_owner_and_unchanged_bank_for_retry() {
    fixture!(|bound, core, state, storage, bank| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let files = file_bytes(&state.lock().unwrap());
        bound = match bound.into_empty_transactions(&mut owner, &mut bank) {
            Err(IntoEmptyTransactionsError::Before { error, bound }) => {
                assert!(matches!(error.cause, Cause::NoStatus));
                assert_eq!(error.effect, Effect::Unchanged);
                bound
            }
            _ => panic!("empty provisioning should return Before"),
        };
        assert!(bank.iter().all(|byte| *byte == 0x6d));
        assert_eq!(file_bytes(&state.lock().unwrap()), files);
        assert!(!storage.session_fence().is_fenced());
        init!(bound, owner);
        assert!(bound.into_empty_transactions(&mut owner, &mut bank).is_ok());
    });
}

#[test]
fn empty_handoff_foreign_owner_preserves_binding_and_bank() {
    fixture!(|bound, core, state, storage, bank, foreign| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        init!(bound, owner);
        let mut other = foreign.try_owner(OwnerRole::Maintenance).unwrap();
        let prefix = storage.wal().prefix().unwrap();
        bound = match bound.into_empty_transactions(&mut other, &mut bank) {
            Err(IntoEmptyTransactionsError::Before { error, bound }) => {
                assert!(
                    matches!(error.cause, Cause::Frame(e) if e.cause == dirty_frame::Cause::BindingMismatch)
                );
                bound
            }
            _ => panic!("foreign owner should return Before"),
        };
        assert!(bank.iter().all(|byte| *byte == 0x6d));
        assert_eq!(storage.wal().prefix().unwrap(), prefix);
        assert!(!storage.session_fence().is_fenced());
        assert!(!state.lock().unwrap().files.is_empty());
        assert!(bound.into_empty_transactions(&mut owner, &mut bank).is_ok());
    });
}

#[test]
fn empty_handoff_retained_flush_claim_blocks_with_zero_pins() {
    fixture!(|bound, core, state, storage, bank| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        init!(bound, owner);
        dirty_frame::test_support::seed_uncaptured_claim(&bound.inner.inner.pool);
        assert!(!bound.inner.inner.pool.has_pins().unwrap());
        match bound.into_empty_transactions(&mut owner, &mut bank) {
            Err(IntoEmptyTransactionsError::Before { error, bound }) => {
                assert!(matches!(error.cause, Cause::Busy));
                assert!(!storage.session_fence().is_fenced());
                drop(bound);
            }
            _ => panic!("retained flush claim should block conversion"),
        }
        assert!(bank.iter().all(|byte| *byte == 0x6d));
        assert!(!state.lock().unwrap().files.is_empty());
    });
}

#[test]
fn empty_handoff_forgotten_actual_capture_keeps_claim_without_pins() {
    fixture!(|bound, core, state, storage, bank| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        init!(bound, owner);
        let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let pool = &bound.inner.inner.pool;
        let request = pool.target_request(0).unwrap();
        let mut image = boxed::<PAGE_BYTES>(0);
        let captured = pool
            .capture_flush(&mut dedicated, request, &mut image)
            .unwrap()
            .unwrap();
        #[allow(
            clippy::forget_non_drop,
            reason = "losing this diagnostic token must not clear its retained DPT claim"
        )]
        std::mem::forget(captured);
        assert!(!pool.has_pins().unwrap());
        match bound.into_empty_transactions(&mut owner, &mut bank) {
            Err(IntoEmptyTransactionsError::Before { error, bound }) => {
                assert!(matches!(error.cause, Cause::Busy));
                assert!(!storage.session_fence().is_fenced());
                drop(bound);
            }
            _ => panic!("forgotten actual capture should block conversion"),
        }
        assert!(bank.iter().all(|byte| *byte == 0x6d));
        assert!(!state.lock().unwrap().files.is_empty());
    });
}

#[test]
fn empty_handoff_poisoned_dpt_fences_before_bank_initialization() {
    fixture!(|bound, core, state, storage, bank| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        init!(bound, owner);
        dirty_frame::test_support::poison_dpt(&bound.inner.inner.pool);
        assert!(matches!(
            bound.into_empty_transactions(&mut owner, &mut bank),
            Err(IntoEmptyTransactionsError::Fatal(_))
        ));
        assert!(bank.iter().all(|byte| *byte == 0x6d));
        assert!(storage.session_fence().is_fenced());
        assert!(!state.lock().unwrap().files.is_empty());
    });
}

#[test]
fn empty_handoff_forgotten_reservation_keeps_bank_and_owner() {
    fixture!(|bound, core, state, storage, bank| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let (mut writer, _, _) = bound.split_with_flush();
        std::mem::forget(writer.reserve_one(&mut owner).unwrap());
        let prefix = storage.wal().prefix().unwrap();
        let result = bound.into_empty_transactions(&mut owner, &mut bank);
        match result {
            Err(IntoEmptyTransactionsError::Before { error, bound }) => {
                assert!(matches!(error.cause, Cause::Busy));
                assert!(!storage.session_fence().is_fenced());
                assert_eq!(storage.wal().prefix().unwrap(), prefix);
                assert!(bound.inner.inner.control.base.batch.is_some());
                drop(bound);
            }
            _ => panic!("forgotten reserve should block conversion"),
        }
        assert!(bank.iter().all(|byte| *byte == 0x6d));
        assert!(!state.lock().unwrap().files.is_empty());
    });
}

#[test]
fn empty_handoff_forgotten_read_admission_blocks_without_pins() {
    fixture!(|bound, core, state, storage, bank| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        init!(bound, owner);
        let (_, reader, _) = bound.split_with_flush();
        std::mem::forget(reader.try_admit(&mut owner).unwrap());
        assert!(!bound.inner.inner.pool.has_pins().unwrap());
        match bound.into_empty_transactions(&mut owner, &mut bank) {
            Err(IntoEmptyTransactionsError::Before { error, bound }) => {
                assert!(matches!(error.cause, Cause::Busy));
                assert!(!storage.session_fence().is_fenced());
                drop(bound);
            }
            _ => panic!("forgotten read gate should block conversion"),
        }
        assert!(bank.iter().all(|byte| *byte == 0x6d));
        assert!(!state.lock().unwrap().files.is_empty());
    });
}

#[test]
fn empty_handoff_forgotten_pool_pins_block_without_read_gate() {
    fixture!(|bound, core, state, storage, bank| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        init!(bound, owner);
        let request = bound.inner.inner.pool.target_request(0).unwrap();
        std::mem::forget(
            bound
                .inner
                .inner
                .pool
                .reserve_target_set(&mut owner, &[request])
                .unwrap(),
        );
        assert!(bound.inner.inner.pool.has_pins().unwrap());
        assert!(bound.read_gate.try_lock().is_ok());
        match bound.into_empty_transactions(&mut owner, &mut bank) {
            Err(IntoEmptyTransactionsError::Before { error, bound }) => {
                assert!(matches!(error.cause, Cause::Busy));
                assert!(!storage.session_fence().is_fenced());
                drop(bound);
            }
            _ => panic!("forgotten pool pin should block conversion"),
        }
        assert!(bank.iter().all(|byte| *byte == 0x6d));
        assert!(!state.lock().unwrap().files.is_empty());
    });
}

#[test]
fn empty_handoff_poisoned_read_gate_fences_without_touching_bank() {
    fixture!(|bound, core, state, storage, bank| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        init!(bound, owner);
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                let _gate = bound.read_gate.lock().unwrap();
                panic!("handoff gate poison");
            }))
            .is_err()
        );
        assert!(matches!(
            bound.into_empty_transactions(&mut owner, &mut bank),
            Err(IntoEmptyTransactionsError::Fatal(EmptyTransactionError {
                effect: Effect::CallerMustResolve,
                ..
            }))
        ));
        assert!(bank.iter().all(|byte| *byte == 0x6d));
        assert!(storage.session_fence().is_fenced());
        assert!(!state.lock().unwrap().files.is_empty());
    });
}

#[test]
fn empty_handoff_corrupt_system_descriptor_fences_and_preserves_bank() {
    fixture!(|bound, core, state, storage, bank| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        init!(bound, owner);
        bound.inner.inner.control.base.descriptor[0] ^= 1;
        let files = file_bytes(&state.lock().unwrap());
        assert!(matches!(
            bound.into_empty_transactions(&mut owner, &mut bank),
            Err(IntoEmptyTransactionsError::Fatal(_))
        ));
        assert!(bank.iter().all(|byte| *byte == 0x6d));
        assert_eq!(file_bytes(&state.lock().unwrap()), files);
        assert!(storage.session_fence().is_fenced());
    });
}

#[test]
fn empty_handoff_fence_during_checked_release_preserves_bank() {
    fixture!(|bound, core, state, storage, bank| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        init!(bound, owner);
        let files = file_bytes(&state.lock().unwrap());
        let (arrived_tx, arrived_rx) = std::sync::mpsc::sync_channel(1);
        let (resume_tx, resume_rx) = std::sync::mpsc::channel();
        dirty_frame::test_support::install_release_pause(
            &bound.inner.inner.pool,
            arrived_tx,
            resume_rx,
        );
        let fence = storage.session_fence();
        std::thread::scope(|scope| {
            let concurrent = scope.spawn(move || {
                arrived_rx
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap();
                fence.fence();
                resume_tx
                    .send(dirty_frame::test_support::Resume::Continue)
                    .unwrap();
            });
            assert!(matches!(
                bound.into_empty_transactions(&mut owner, &mut bank),
                Err(IntoEmptyTransactionsError::Fatal(_))
            ));
            concurrent.join().unwrap();
        });
        assert!(bank.iter().all(|byte| *byte == 0x6d));
        assert_eq!(file_bytes(&state.lock().unwrap()), files);
        assert!(storage.session_fence().is_fenced());
    });
}

#[test]
fn empty_handoff_preexisting_fence_never_returns_usable_owner() {
    fixture!(|bound, core, state, storage, bank| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        init!(bound, owner);
        storage.session_fence().fence();
        assert!(matches!(
            bound.into_empty_transactions(&mut owner, &mut bank),
            Err(IntoEmptyTransactionsError::Fatal(_))
        ));
        assert!(bank.iter().all(|byte| *byte == 0x6d));
        assert!(storage.session_fence().is_fenced());
        assert!(!state.lock().unwrap().files.is_empty());
    });
}

fn expanded_handoff(profile: u32, count: usize) {
    let mut wal_slot = port::slot();
    let mut workspace = boxed::<{ 4 << 20 }>(0);
    let (namespace, state) = port::fresh();
    let fresh = FreshStorageSession::create_status(
        namespace,
        profile,
        &mut wal_slot,
        &mut [[0; PAGE_BYTES]; 3],
    )
    .unwrap();
    let installed = fresh.install_doublewrite(&mut workspace).unwrap();
    let storage = installed.into_mutable_storage();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut images: Vec<_> = (0..count).map(|_| boxed::<PAGE_BYTES>(0xa7)).collect();
    let mut frames: Vec<_> = images
        .iter_mut()
        .map(|bytes| DirtyFrame::new(bytes))
        .collect();
    let mut operation = boxed::<114688>(0);
    let binding = storage
        .bind_resident_status(&core, &mut frames, &mut operation)
        .unwrap();
    let mut bound = match binding.activate() {
        Ok(value) => value,
        Err(_) => panic!("activation"),
    };
    let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
    for index in 1..count {
        bound.prepare_slot(index).unwrap();
        let observed = init!(bound, owner);
        assert_eq!(
            (observed.page.page_no, observed.lsn.0),
            (index as u64, 2 * index as u64)
        );
    }
    let files = file_bytes(&state.lock().unwrap());
    let mut bank = boxed::<EMPTY_TRANSACTION_BANK_BYTES>(0x6d);
    let value = match bound.into_empty_transactions(&mut owner, &mut bank) {
        Ok(value) => value,
        Err(_) => panic!("handoff"),
    };
    assert_eq!(
        value.snapshot().unwrap().frozen_system_lsn.0,
        2 * (count as u64 - 1)
    );
    assert_eq!(file_bytes(&state.lock().unwrap()), files);
    drop(value);
    assert!(storage.session_fence().is_fenced());
}
#[test]
#[ignore = "DEV3 S5: expanded profile/frame-size handoff inventory"]
fn empty_handoff_s5_profiles_and_resident_capacities() {
    for profile in 1..=3 {
        for count in [2, 3, 8, 64] {
            expanded_handoff(profile, count);
        }
    }
}
