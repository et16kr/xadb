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
fn catalog_commit_phase_sources_measure_consumed_preparation() {
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
    // Actual preparation arrays end at stage return. The compact image keeps
    // only immutable install/control input; this is not a writer admission.
    let measured = |key| phases.iter().find(|(name, _)| *name == key).unwrap().1;
    assert!(measured("pending_image") + 1024 <= measured("prepared_actual_nine"));
    assert!(shared + measured("pending_wal_wait") + resident * 2 <= 65536);
    assert!(shared + measured("production_broad") <= 65536);
}

#[test]
fn catalog_aging_source_phases_include_initial_and_malformed_paths() {
    use crate::storage::{obligation, retention_obligation, retention_page, transaction_record};
    let root = retention_page::catalog::root_decode_source_bytes();
    let queue = retention_page::catalog::source_bytes();
    let descriptor = obligation::descriptor_source_bytes();
    let transaction = transaction_record::status_codec_source_bytes();
    let aging = retention_obligation::catalog::source_bytes();
    let shared = source::shared_bytes::<port::Writer>();
    let full = read::lookup_source_bytes::<port::Writer>();
    let resident = read::resident_lookup_source_bytes::<port::Writer>();
    let phases = create_pair::catalog_commit_phase_observations::<port::Writer>();
    let wait = phases
        .iter()
        .find(|(key, _)| *key == "pending_wal_wait")
        .unwrap()
        .1;
    println!(
        "AGING_SOURCE root={root} whole_queue={queue} descriptor={descriptor} transaction={transaction} aging={aging} full={full} resident={resident} shared={shared} pending_wait={wait} full_overlap={}",
        shared + full + wait
    );
    // Initial Root can nest two canonical Root buffers and the initial Job;
    // malformed XOD Transaction bytes reach the fixed status decoder.
    assert!(root >= 2 * 512 + 1024);
    assert!(descriptor > transaction + 1536);
    assert!(aging >= root && aging >= descriptor);
    assert!(shared + full <= 65536);
    assert!(shared + resident * 2 <= 65536);
}

#[test]
fn catalog_wal_wait_source_phases_include_claim_and_condvar() {
    let physical = crate::storage::wal_io::physical::wait_source_observations::<port::Writer>();
    for (name, bytes) in physical {
        println!("WAL_WAIT {name}={bytes}");
    }
    let [parent, bridge, handoff, pending] =
        create_pair::catalog_wait_source_observation::<port::Writer>();
    let shared = source::shared_bytes::<port::Writer>();
    let full = read::lookup_source_bytes::<port::Writer>();
    let resident = read::resident_lookup_source_bytes::<port::Writer>();
    println!(
        "WAL_WAIT parent={parent} bridge={bridge} handoff={handoff} pending={pending} shared={shared} full={full} full_overlap={} two_resident_overlap={}",
        shared + full + pending,
        shared + 2 * resident + pending
    );
    let measured = |name| physical.iter().find(|(key, _)| *key == name).unwrap().1;
    assert!(measured("ensure_durable") > measured("sync_claim"));
    assert!(measured("wait_durable") > measured("ensure_durable"));
    assert!(measured("wait_durable") > measured("condvar"));
    assert!(bridge > measured("wait_durable"));
    assert!(pending > bridge);
    // Whether a full reader fits is a reported fact, not a relaxed criterion.
    // Entry/exit/source-ticket carriers and the actual phase protocol remain.
}

#[test]
fn catalog_call_scope_selected_consumer_source_is_bounded() {
    let phases = read::catalog_call_source_observations::<port::Writer>();
    let shared = source::shared_bytes::<port::Writer>();
    println!(
        "C2_CALL_SCOPE held={} entry={} consumer={} full={} resident={} shared={}",
        phases[0], phases[1], phases[2], phases[3], phases[4], shared
    );
    assert!(phases[2] < phases[4] && phases[4] < phases[3]);
    assert!(shared + phases[3] <= 65_536);
    assert!(shared + 2 * phases[4] <= 65_536);
}

#[test]
fn catalog_replay_sources_exclude_live_encoder_arrays() {
    use crate::storage::wal_io::bootstrap::doublewrite_install::allocation_reservation::operation;
    use crate::storage::wal_io::bootstrap::doublewrite_install::mutable_storage::status_init_operation::retention as init;
    let branches = operation::retention_readonly_source_branches();
    let base = operation::plan4_system_base_allocation_source_bytes();
    let reservation = operation::retention::retained_reservation_source_bytes();
    let materialize = operation::retention::retention_materialize_source_bytes();
    let init = init::retention_materialize_source_bytes();
    let full = read::lookup_source_bytes::<port::Writer>();
    let resident = read::resident_lookup_source_bytes::<port::Writer>();
    let shared = source::shared_bytes::<port::Writer>();
    println!(
        "C2_REPLAY_SOURCES branches={branches:?} base={base} reservation={reservation} materialize={materialize} init={init} full={full} resident={resident} shared={shared}"
    );
    // Decoding retains malformed XTS and expected construction retains the
    // original current descriptor. Only actual sequential phases use max.
    assert!(branches[1] > crate::storage::obligation::descriptor_source_bytes());
    assert!(branches[2] >= 4 * std::mem::size_of::<crate::storage::obligation::DescriptorSpec>());
    assert!(materialize > reservation);
    assert!(shared + full <= 65_536);
    assert!(shared + 2 * resident <= 65_536);
}

#[test]
fn catalog_borrowed_replay_workspace_counts_producer_and_observer() {
    use crate::storage::semantic_catalog as semantic;
    let owned = semantic::source_census();
    let borrowed = semantic::borrowed_source_bytes();
    let history = history::source_bytes::<port::Writer>();
    let full = read::lookup_source_bytes::<port::Writer>();
    let resident = read::resident_lookup_source_bytes::<port::Writer>();
    let shared = source::shared_bytes::<port::Writer>();
    println!(
        "BORROWED_REPLAY_SOURCE owned={owned:?} borrowed={borrowed} history={history:?} full={full} resident={resident} shared={shared}"
    );
    assert!(borrowed < owned.total);
    assert!(
        borrowed > owned.total - owned.run_retained,
        "borrowed loan/constructor/cleanup carriers remain charged"
    );
    assert!(history.peak >= history.initialize);
    assert!(shared + full <= 65536);
    assert!(shared + 2 * resident <= 65536);
}
