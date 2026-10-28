//! New shared start entries: capacity precedes issued-local allocation, and
//! original malformed inputs/roles keep WAL and private bytes unchanged.
use super::*;
use crate::storage::disk::{RetentionCause, RetentionPhase};
use crate::transaction::concurrency::catalog_test_support as hook;

#[test]
fn shared_start_capacity_roles_and_begin() {
    case("MCSP01", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            observe_start = true,
            |live,
             owner,
             flusher,
             status,
             undo,
             private,
             reader_private,
             core,
             fence,
             state,
             foreign| {
                let _ = reader_private;
                prepare_create(live, owner, flusher, status, undo);
                let before = state.lock().unwrap().images();
                let private_before = hex(private);
                let wrong = live.target(Plan4Page::Root).unwrap();
                assert_eq!(live.catalog_test_locals(), (0, 0));
                {
                    let (mut writer, reader, mut flush) = live.split();
                    let [shared, ..] = writer.catalog_test_commit_charges();
                    let held = 65536 - shared - writer.catalog_test_retirement_charge() + 1;
                    reader.catalog_test_with_held_source(held, || {
                        for aging in [false, true] {
                            let error = if aging {
                                writer.catalog_aging_start(owner, private, &mut flush)
                            } else {
                                writer.catalog_begin(owner, status, private, &mut flush)
                            }
                            .err()
                            .unwrap();
                            assert_eq!(
                                (error.phase, error.effect),
                                (RetentionPhase::Census, Effect::Unchanged)
                            );
                            assert!(matches!(error.cause, RetentionCause::Capacity));
                            assert_eq!(reader.catalog_test_source_used(), held as u64);
                            assert!(!hook::observe(core).read_gate_closed);
                        }
                    });
                    assert_eq!(hex(private), private_before);
                    let error = writer
                        .catalog_begin(owner, &wrong, private, &mut flush)
                        .err()
                        .unwrap();
                    assert_eq!(
                        (error.phase, error.effect),
                        (RetentionPhase::Prepare, Effect::Unchanged)
                    );
                    assert!(matches!(error.cause, RetentionCause::Membership));
                    let error = writer
                        .catalog_begin(flusher, status, private, &mut flush)
                        .err()
                        .unwrap();
                    assert!(matches!(error.cause, RetentionCause::Concurrency(error)
                    if error.cause == crate::transaction::concurrency::Cause::InvalidRole));
                    let mut alien = foreign.try_owner(OwnerRole::Foreground).unwrap();
                    let error = writer
                        .catalog_begin(&mut alien, status, private, &mut flush)
                        .err()
                        .unwrap();
                    assert!(matches!(error.cause, RetentionCause::Concurrency(error)
                    if error.cause == crate::transaction::concurrency::Cause::InvalidRole));
                    alien.check_idle().unwrap();
                    assert_eq!(reader.catalog_test_source_used(), 0);
                    assert!(!hook::observe(core).read_gate_closed);
                    assert_eq!(writer.catalog_test_physical_pins(), 0);
                    assert_eq!(writer.catalog_test_references(), ((0, 0, 0), false));
                }
                assert_eq!(live.catalog_test_locals(), (0, 0));
                assert_eq!(state.lock().unwrap().images(), before);
                assert_eq!(hex(private), private_before);
                let mut maintenance = core.try_owner(OwnerRole::Maintenance).unwrap();
                let begin =
                    observe_start(live, &mut maintenance, private, core, state, Some(status));
                assert_eq!(begin.actor, Some(oracle.actor("R01")));
                assert_eq!(begin.header.lsn, 6);
                maintenance.check_idle().unwrap();
                drop(maintenance);
                oracle.check(live, state, "R01");
                assert_eq!(live.catalog_test_locals(), (1, 1));
                let occupied_images = state.lock().unwrap().images();
                let occupied_private = hex(private);
                {
                    let (mut writer, reader, mut flush) = live.split();
                    let error = writer
                        .catalog_begin(owner, status, private, &mut flush)
                        .err()
                        .unwrap();
                    assert_eq!(
                        (error.phase, error.effect),
                        (RetentionPhase::Prepare, Effect::Unchanged)
                    );
                    assert!(matches!(error.cause, RetentionCause::WrongState));
                    assert_eq!(reader.catalog_test_source_used(), 0);
                    assert!(!hook::observe(core).read_gate_closed);
                }
                assert_eq!(live.catalog_test_locals(), (1, 1));
                assert_eq!(hex(private), occupied_private);
                assert_eq!(state.lock().unwrap().images(), occupied_images);
                assert!(!fence.is_fenced());
                owner.check_idle().unwrap();
                flusher.check_idle().unwrap();
            }
        );
    });
}

/// Observe each start action's actual successful preparation/application, without
/// changing its candidate, expected output, or original source/gate ownership.
pub(super) fn observe_start<'s, 'c>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, port::Writer>,
    owner: &mut Owner<'c, 's>,
    private: &mut [u8; PRIVATE_BYTES],
    core: &PageConcurrency<'s>,
    state: &State,
    status: Option<&Plan4Target>,
) -> crate::storage::semantic_catalog::Observation {
    use crate::transaction::concurrency::catalog_test_support::{Point, Resume};
    use std::sync::mpsc::sync_channel;
    let before = state.lock().unwrap().images();
    let (mut writer, reader, mut flush) = live.split();
    let charge = writer.catalog_test_retirement_charge() as u64;
    let (entered, preparing) = sync_channel(1);
    let (resume_prepare, release) = sync_channel(1);
    hook::install(core, Point::LifecyclePreparing, entered, release);
    let observation = std::thread::scope(|scope| {
        let reader_ref = &reader;
        let controlling = scope.spawn(move || {
            preparing.recv_timeout(WAIT).unwrap();
            assert!(hook::observe(core).read_gate_closed);
            assert_eq!(reader_ref.catalog_test_source_used(), charge);
            assert_eq!(state.lock().unwrap().images(), before);
            let (entered, prepared) = sync_channel(1);
            let (resume_prepared, release) = sync_channel(1);
            hook::install(core, Point::LifecyclePrepared, entered, release);
            resume_prepare.send(Resume::Continue).unwrap();
            prepared.recv_timeout(WAIT).unwrap();
            assert!(hook::observe(core).read_gate_closed);
            assert_eq!(reader_ref.catalog_test_source_used(), charge);
            assert_eq!(state.lock().unwrap().images(), before);
            let (entered, completed) = sync_channel(1);
            let (resume_completed, release) = sync_channel(1);
            hook::install(core, Point::LifecycleCompleted, entered, release);
            resume_prepared.send(Resume::Continue).unwrap();
            completed.recv_timeout(WAIT).unwrap();
            assert!(hook::observe(core).read_gate_closed);
            assert_eq!(reader_ref.catalog_test_source_used(), charge);
            assert_ne!(state.lock().unwrap().images(), before);
            resume_completed.send(Resume::Continue).unwrap();
        });
        let result = if let Some(status) = status {
            writer.catalog_begin(owner, status, private, &mut flush)
        } else {
            writer.catalog_aging_start(owner, private, &mut flush)
        }
        .unwrap();
        controlling.join().unwrap();
        result
    });
    assert_eq!(reader.catalog_test_source_used(), 0);
    assert!(!hook::observe(core).read_gate_closed);
    assert_eq!(writer.catalog_test_physical_pins(), 0);
    assert_eq!(writer.catalog_test_references(), ((0, 0, 0), false));
    observation
}
