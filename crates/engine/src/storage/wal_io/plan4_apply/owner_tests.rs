//! Canonical owner transfer only. Existing lifecycle suites are not invoked.
use super::*;
use crate::{
    storage::{
        dirty_frame::DirtyFrame,
        wal_frame::MAX_FRAME_BYTES,
        wal_io::{FreshStorageSession, plan4_test_support as port},
    },
    transaction::concurrency::{OwnerRole, PageConcurrency},
};
use foundation::namespace::DirectoryPort;

fn boxed<const N: usize>() -> Box<[u8; N]> {
    vec![0; N].into_boxed_slice().try_into().unwrap()
}

fn canonical_transfer(reject_foreign_owner: bool) {
    let directory = port::Directory::fresh();
    let state = directory.state();
    state.lock().unwrap().live_writes = true;
    let mut slot = boxed::<MAX_FRAME_BYTES>();
    let slot_pointer = slot.as_ptr();
    let mut maintenance = boxed::<12_582_912>();
    let mut dwb = boxed::<4_194_304>();
    let fresh = FreshStorageSession::create_retention_catalog(
        directory.acquire_writer().unwrap(),
        1,
        &mut slot,
        &mut maintenance,
    )
    .unwrap();
    let storage = fresh
        .install_doublewrite(&mut dwb)
        .unwrap()
        .into_mutable_storage();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let foreign = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
    let mut foreign_owner = foreign.try_owner(OwnerRole::Foreground).unwrap();
    let mut backing = vec![[0xa7; PAGE_BYTES]; 4];
    let mut frames: Vec<_> = backing.iter_mut().map(DirtyFrame::new).collect();
    let mut operation = boxed::<114_688>();
    let operation_pointer = operation.as_ptr();
    let mut isolation = boxed::<4_160>();
    isolation.fill(0xa7);
    let isolation_pointer = isolation.as_ptr();
    let mut control = boxed::<RETENTION_CONTROL_BYTES>();
    let control_pointer = control.as_ptr();
    let mut replay = boxed::<RETENTION_REPLAY_BYTES>();
    let replay_pointer = replay.as_ptr();
    let old = storage
        .bind_plan4_pages(&core, &mut frames, &mut operation)
        .unwrap();
    let wal_pointer = storage.wal() as *const _;
    let fence_pointer = storage.session_fence() as *const _;
    let prefix = storage.wal().prefix().unwrap();
    let before = {
        let state = state.lock().unwrap();
        (
            state.acquire_calls,
            state.entropy_calls,
            state.mutations(),
            state.writers,
            state.handles,
        )
    };
    let old = if reject_foreign_owner {
        match old.into_disk_session(
            &mut foreign_owner,
            EmptyRetentionControlLoan {
                isolation: &mut isolation,
                control: &mut control,
                replay: &mut replay,
            },
        ) {
            Err(IntoRetentionError::Before { error, bound }) => {
                assert_eq!(error.effect, Effect::Unchanged);
                assert!(std::ptr::eq(bound.storage, &storage));
                assert!(std::ptr::eq(bound.pool.bound_core(), &core));
                assert_eq!(bound.operation.as_ptr(), operation_pointer);
                assert!(bound._flush_workspace.is_some());
                assert!(!storage.session_fence().is_fenced());
                bound
            }
            _ => panic!("foreign owner must return the original healthy Plan4Pages"),
        }
    } else {
        old
    };
    assert!(isolation.iter().all(|byte| *byte == 0xa7));
    let mut session: crate::storage::disk::DiskSession<'_, '_, '_, '_, '_, '_, '_, '_, _> =
        match old.into_disk_session(
            &mut owner,
            EmptyRetentionControlLoan {
                isolation: &mut isolation,
                control: &mut control,
                replay: &mut replay,
            },
        ) {
            Ok(session) => session,
            Err(_) => panic!("canonical conversion failed"),
        };
    assert!(std::ptr::eq(session.inner.storage, &storage));
    assert!(std::ptr::eq(session.inner.pool.bound_core(), &core));
    assert_eq!(session.inner.storage.wal() as *const _, wal_pointer);
    assert_eq!(
        session.inner.storage.session_fence() as *const _,
        fence_pointer
    );
    assert_eq!(session.inner.operation.as_ptr(), operation_pointer);
    assert!(session.inner._flush_workspace.is_none());
    assert_eq!(session.control.ledger.bytes.as_ptr(), control_pointer);
    assert_eq!(
        session.history.exclusive().unwrap().as_ptr(),
        replay_pointer
    );
    assert_eq!(
        storage
            .wal()
            .append
            .lock()
            .unwrap()
            .slot
            .as_ref()
            .unwrap()
            .as_ptr(),
        slot_pointer
    );
    assert_eq!(storage.wal().prefix().unwrap(), prefix);
    {
        let isolation = session.control.isolation.lock().unwrap();
        assert_eq!(isolation.bytes.as_ptr(), isolation_pointer);
        assert_eq!(&isolation.bytes[..8], &1_u64.to_le_bytes());
        assert!(isolation.bytes[8..].iter().all(|byte| *byte == 0));
        assert!(std::ptr::eq(isolation.core, &core));
        assert!(std::ptr::eq(isolation.binding, &storage.binding));
        assert_eq!(isolation.fence as *const _, fence_pointer);
    }
    let isolation_mutex = &session.control.isolation as *const _;
    let pool_pointer = &session.inner.pool as *const _;
    let history_pointer = &session.history as *const _;
    let ledger_pointer = &session.control.ledger as *const _;
    let flush_pointer = &session.flush as *const _;
    {
        let (mutation, read, maintenance) = session.split();
        assert_eq!(mutation.pool as *const _, pool_pointer);
        assert_eq!(read.pool as *const _, pool_pointer);
        assert_eq!(maintenance.pool as *const _, pool_pointer);
        assert_eq!(mutation.history as *const _, history_pointer);
        assert_eq!(read.history as *const _, history_pointer);
        assert_eq!(mutation.control as *const _, ledger_pointer);
        assert_eq!(mutation.isolation as *const _, isolation_mutex);
        assert_eq!(read.isolation as *const _, isolation_mutex);
        // The writer's exclusive durable borrow coexists with the shared mutex.
        let isolation = read.isolation.try_lock().unwrap();
        assert_eq!(isolation.bytes.as_ptr(), isolation_pointer);
        assert_eq!(mutation.control.bytes.as_ptr(), control_pointer);
        assert!(mutation.isolation.try_lock().is_err());
        drop(isolation);
        assert_eq!(maintenance.control as *const _, flush_pointer);
        assert_eq!(mutation.operation.as_ptr(), operation_pointer);
        for port_storage in [mutation.storage, read.storage, maintenance.storage] {
            assert!(std::ptr::eq(port_storage, &storage));
        }
    }
    let after = {
        let state = state.lock().unwrap();
        (
            state.acquire_calls,
            state.entropy_calls,
            state.mutations(),
            state.writers,
            state.handles,
        )
    };
    assert_eq!(
        after, before,
        "conversion/split must not replace resources or mutate storage"
    );
    drop(session);
    assert!(storage.session_fence().is_fenced());
    assert_eq!(state.lock().unwrap().mutations(), before.2);
    drop(owner);
    drop(foreign_owner);
    drop(storage);
    let state = state.lock().unwrap();
    assert_eq!((state.writers, state.handles), (0, 0));
}

#[test]
fn disk_session_moves_original_resources_and_splits_disjoint_ports() {
    canonical_transfer(false);
}

#[test]
fn disk_session_before_returns_original_owner_for_canonical_retry() {
    canonical_transfer(true);
}

#[test]
fn disk_session_isolation_backing_and_carriers_are_accounted() {
    let live = resources::source_census::<port::Writer>();
    let recovered = super::super::super::plan4_recovery::source_census::<port::Writer>();
    let directory = dirty_frame::plan4_core_census().directory;
    println!(
        "C2_SEMANTIC_BRANCHES {:?}",
        crate::storage::semantic_catalog::source_branches()
    );
    println!("C1_ISOLATION_LIVE {live:?}");
    println!("C1_ISOLATION_RECOVERED {recovered:?}");
    assert_eq!(live.caller_isolation, 4_160);
    assert_eq!(visibility::source_census().backing, 9_792);
    assert!(visibility::fits());
    assert!(resources::fits::<port::Writer>());
    assert!(recovered.peak <= 65_536);
    assert!(recovered.metadata <= 196_608);
    assert!(recovered.whole <= 262_144);
    assert_eq!(
        live.metadata - live.peak,
        73_856 + transaction::LONG_SOURCE_BYTES
    );
    let handles = transaction::handle_census();
    println!(
        "C1_READ_ONLY_HANDLES {handles:?} short={}",
        transaction::source_bytes()
    );
    assert!(handles.0 <= 512 && handles.1 <= 512);
    assert_eq!(
        recovered.metadata - recovered.retained_live - directory,
        79_488
    );
    assert!(
        std::mem::size_of::<RetentionControl<'_, '_, '_>>()
            >= std::mem::size_of::<control::RetentionLedger<'_, '_, '_>>()
                + std::mem::size_of::<Mutex<IsolationLedger<'_, '_, '_>>>()
    );
}

#[test]
fn catalog_commit_phase_sources_expose_full_reader_conflict() {
    let shared = source::shared_bytes::<port::Writer>();
    let full = read::lookup_source_bytes::<port::Writer>();
    let resident = read::resident_lookup_source_bytes::<port::Writer>();
    let phases = create_pair::catalog_commit_phase_observations::<port::Writer>();
    for (name, bytes) in phases {
        println!(
            "COMMIT_PHASE {name}={bytes} shared_sum={} full_overlap={} resident_overlap={} two_resident_overlap={}",
            shared + bytes,
            shared + bytes + full,
            shared + bytes + resident,
            shared + bytes + resident * 2
        );
    }
    // Three real values alone already rule out simply bolting a fixed Commit
    // ticket onto the current Full lookup. Do not weaken the 64KiB envelope.
    let floor = phases
        .iter()
        .find(|(name, _)| *name == "pending_three_value_floor")
        .unwrap()
        .1;
    let wait = phases
        .iter()
        .find(|(name, _)| *name == "pending_wal_wait")
        .unwrap()
        .1;
    assert!(shared + floor + full > 65536);
    assert!(shared + wait + resident <= 65536);
    assert_eq!(
        phases.last().unwrap().1,
        31752,
        "existing production bound must not silently change in an observation-only unit"
    );
}
