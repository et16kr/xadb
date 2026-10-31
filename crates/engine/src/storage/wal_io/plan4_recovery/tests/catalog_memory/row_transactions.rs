//! Row Begin and the empty Row abort through the actual writer
//! (XADB-MODULE-ROW-BEGIN-001 and WRITABLE-EMPTY-001 for a Row capsule).
//! Expected values are the original Catalog corpus where the selection is
//! shared (actor, slot, LSN) and the actual replay of the same WAL where the
//! Row state is new: `history::validate_live` proves live == replay and the
//! fixture compares the replayed ATT rows and Job cells with the live ones.
use super::super::faults::{rejected, unchanged_error};
use super::*;
use crate::storage::disk::{
    IsolationLevel, RetentionCause, RetentionPhase, StatementFinishError, TransactionFinishError,
    TransactionOptions,
};
use crate::storage::retention_page::row::{self as job3, JobScope, JobState};
use crate::transaction::concurrency::catalog_test_support::{self as hook, Point, Resume};
use std::sync::mpsc::sync_channel;

const ATT_ACTOR: std::ops::Range<usize> = 1576..1624;

fn namespace_of(actor: TxnRef) -> crate::api::Namespace {
    crate::api::Namespace {
        database: actor.id().database,
        timeline: actor.id().timeline,
    }
}

/// The one Job cell of `actor`; it must carry the Row scope.
pub(super) fn row_job(jobs: &[[u8; 1024]; 64], actor: TxnRef) -> (usize, JobState, u8, u64) {
    let mut found = None;
    for (ordinal, bytes) in jobs.iter().enumerate() {
        let job = job3::decode_job(bytes, namespace_of(actor)).unwrap();
        if job.source() == Some(actor) {
            assert_eq!(job.scope(), Some(JobScope::Row));
            let cell = (ordinal, job.state(), job.outcome(), job.commit_q());
            assert!(found.replace(cell).is_none(), "one Row Terminal");
        }
    }
    found.expect("Row Terminal")
}

/// Live == replay through the original `history::validate_live`, plus the
/// fixture's own byte comparison of the replayed ATT rows and Job cells.
macro_rules! live_equals_replay {
    ($writer:expr, $owner:expr) => {{
        let (checked, copies) = $writer.row_test_live_replay($owner);
        checked.unwrap();
        assert_eq!(copies[0], copies[2], "replayed ATT rows");
        assert_eq!(copies[1], copies[3], "replayed Job cells");
        copies
    }};
}

/// Restart over an image holding a Row group stops at the interim analysis
/// gate: Unsupported/CallerMustResolve, database Unchanged, every file kept.
fn gated(images: &Images) {
    let directory = clone_directory(images);
    let state = directory.state();
    rejected(&directory, &state, |error, observation| {
        unchanged_error(error, observation, Plan4RecoveryPhase::Analysis);
        assert!(matches!(error.cause, Plan4RecoveryCause::Unsupported));
    });
    assert_eq!(
        &state.lock().unwrap().images(),
        images,
        "every file unchanged"
    );
}

/// A second preprovisioned Status (page 11) by the original exclusive INIT.
pub(super) fn second_status<'s, 'c, W: WriterNamespace>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
    owner: &mut Owner<'c, 's>,
    flusher: &mut Owner<'c, 's>,
) -> Plan4Target {
    let mut cursor = Plan4ResidencyCursor::new();
    for _ in 0..32 {
        match live.prepare_status_slot(owner, &mut cursor).unwrap() {
            None => break,
            Some(RetentionResidencyStep::FlushRequired { target, .. }) => {
                drive_flush(live, flusher, Some(&target))
            }
            Some(_) => panic!("unexpected Status preparation"),
        }
    }
    let reserved = live.reserve_status(owner).unwrap();
    assert_eq!(reserved.page(), page_ref(11, 2));
    match reserved.initialize() {
        Ok(target) => target,
        Err(RetentionReservationError::Before { error, .. })
        | Err(RetentionReservationError::Fatal(error)) => panic!("actual INIT {error:?}"),
    }
}

#[test]
fn row_begin_binds_kind_two_and_live_equals_replay() {
    case("R1D-R01", || {
        let oracle = Oracle::new();
        let images = with_fresh!(oracle, |live,
                                          owner,
                                          flusher,
                                          status,
                                          undo,
                                          private,
                                          reader_private,
                                          core,
                                          fence,
                                          state| {
            let _ = reader_private;
            prepare_create(live, owner, flusher, status, undo);
            let epochs = live.catalog_test_epochs();
            let images = {
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                // Shared Begin selection: the Catalog corpus actor/slot/LSN.
                let identity = tx.test_write_identity();
                assert_eq!(identity, (Some(oracle.actor("R01")), 0, 1, 1, 512));
                assert_eq!(tx.test_write_kind(), (2, 2));
                let actor = identity.0.unwrap();
                assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, 6);
                let jobs = writer.catalog_test_jobs();
                assert_eq!(row_job(jobs, actor), (0, JobState::TerminalLeased, 0, 0));
                let bank = writer.recovery_test_bank();
                assert_eq!(TxnRef::decode(&bank[ATT_ACTOR]).ok(), Some(actor));
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert!(!hook::observe(core).read_gate_closed);
                // A5: the actual replay of the same WAL equals the live state.
                let copies = live_equals_replay!(writer, owner);
                assert_eq!(TxnRef::decode(&copies[0][ATT_ACTOR]).ok(), Some(actor));
                let images = state.lock().unwrap().images();
                assert!(
                    writer
                        .abort_empty_transaction(owner, tx, private, &mut flush)
                        .is_ok()
                );
                assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, 8);
                live_equals_replay!(writer, owner);
                images
            };
            // Row Begin and its abort publish no epoch or Q.
            assert_eq!(live.catalog_test_epochs(), epochs);
            assert_eq!(live.catalog_test_q(), 0);
            assert_eq!(live.catalog_test_isolation(), (2, 0, 0));
            assert!(!fence.is_fenced());
            owner.check_idle().unwrap();
            (images, state.lock().unwrap().images())
        });
        // Restart over Row Begin, then over the TerminalReady Row abort.
        gated(&images.0);
        gated(&images.1);
    });
}

#[test]
fn row_begin_rejections_return_original_slot_snapshot_and_wal() {
    case("R1D-R02", || {
        let oracle = Oracle::new();
        with_fresh!(oracle, |live,
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
            let wrong = live.target(Plan4Page::Root).unwrap();
            {
                let (mut writer, reader, mut flush) = live.split();
                for chosen in [&wrong, status] {
                    let selected_owner = if std::ptr::eq(chosen, status) {
                        &mut *flusher
                    } else {
                        &mut *owner
                    };
                    let error = writer
                        .begin_row_transaction(
                            selected_owner,
                            IsolationLevel::Serializable,
                            chosen,
                            private,
                            &mut flush,
                        )
                        .err()
                        .unwrap();
                    assert_eq!(error.effect, Effect::Unchanged);
                    // ADV-04: the cause pins the refusal order (:362).
                    if std::ptr::eq(chosen, status) {
                        // The flusher owner is not a Foreground role.
                        assert_eq!(error.phase, RetentionPhase::Census);
                        assert!(matches!(error.cause, RetentionCause::Visibility(error)
                            if error.cause == super::super::super::super::plan4_pages::VisibilityCause::Concurrency(
                                crate::transaction::concurrency::Cause::InvalidRole)));
                    } else {
                        // The Root target is not a Status page.
                        assert_eq!(error.phase, RetentionPhase::Prepare);
                        assert!(matches!(error.cause, RetentionCause::Membership));
                    }
                    assert_eq!(reader.catalog_test_references().0.0, 0);
                }
                let mut alien = foreign.try_owner(OwnerRole::Foreground).unwrap();
                let error = writer
                    .begin_row_transaction(
                        &mut alien,
                        IsolationLevel::Serializable,
                        status,
                        private,
                        &mut flush,
                    )
                    .err()
                    .unwrap();
                assert_eq!(error.effect, Effect::Unchanged);
                // ADV-04: an Owner of another core is a binding mismatch.
                assert_eq!(error.phase, RetentionPhase::Census);
                assert!(matches!(error.cause, RetentionCause::Frame(error)
                    if error.cause == crate::storage::dirty_frame::Cause::BindingMismatch));
                // RowProgress reserves the one CatalogProgressCharge: one byte
                // short of it beside a held loan is Capacity before Begin.
                let held = 65536
                    - writer.catalog_test_commit_charges()[0]
                    - writer.catalog_test_retirement_charge()
                    + 1;
                reader.catalog_test_with_held_source(held, || {
                    let error = writer
                        .begin_row_transaction(
                            owner,
                            IsolationLevel::Serializable,
                            status,
                            private,
                            &mut flush,
                        )
                        .err()
                        .unwrap();
                    assert!(matches!(error.cause, RetentionCause::Capacity));
                    assert_eq!(error.effect, Effect::Unchanged);
                    assert_eq!(reader.catalog_test_source_used(), held as u64);
                    assert!(!hook::observe(core).read_gate_closed);
                });
                let mut occupied = Vec::new();
                for _ in 0..32 {
                    occupied.push(
                        writer
                            .begin_transaction(
                                owner,
                                TransactionOptions {
                                    read_only: true,
                                    isolation: IsolationLevel::ReadCommitted,
                                },
                                private,
                            )
                            .unwrap(),
                    );
                }
                assert!(matches!(
                    writer
                        .begin_row_transaction(
                            owner,
                            IsolationLevel::ReadCommitted,
                            status,
                            private,
                            &mut flush
                        )
                        .err()
                        .unwrap()
                        .cause,
                    RetentionCause::Capacity
                ));
                drop(occupied);
                assert_eq!(writer.catalog_test_source_used(), 0);
                assert!(!hook::observe(core).read_gate_closed);
            }
            assert_eq!(state.lock().unwrap().images(), before);
            assert_eq!(live.catalog_test_isolation().1, 0);
            assert_eq!(live.catalog_test_isolation().2, 0);
            assert_eq!(live.catalog_test_locals(), (0, 0));
            assert!(!fence.is_fenced());
            // An exhausted Root counter refuses after the issued local: the
            // gap stays a live scalar and the next Begin takes local 2.
            let index = resident_index(live, 1).unwrap();
            let mut bytes = boxed::<PAGE_BYTES>();
            let original = probe::diagnose(live.catalog_test_pool(), index, &mut bytes);
            let mut max_entry = original.entry;
            if let Some(entry) = &mut max_entry {
                entry.modification_counter = u64::MAX;
            }
            probe::seed_counter(live.catalog_test_pool(), index, u64::MAX);
            probe::seed_dpt(live.catalog_test_pool(), index, max_entry);
            let prefix = live.catalog_test_wal_state();
            let before_bank = live.recovery_test_bank().to_vec();
            {
                let (mut writer, _, mut flush) = live.split();
                let error = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        status,
                        private,
                        &mut flush,
                    )
                    .err()
                    .unwrap();
                assert!(matches!(error.cause, RetentionCause::Exhausted));
                assert_eq!(error.effect, Effect::Unchanged);
            }
            assert!(!fence.is_fenced());
            assert_eq!(live.catalog_test_locals(), (1, 0));
            assert_eq!(live.catalog_test_wal_state(), prefix);
            assert_eq!(live.recovery_test_bank(), before_bank);
            assert_eq!(live.catalog_test_isolation().1, 0);
            assert_eq!(state.lock().unwrap().images(), before);
            probe::seed_counter(
                live.catalog_test_pool(),
                index,
                original.frame.modification_counter,
            );
            probe::seed_dpt(live.catalog_test_pool(), index, original.entry);
            {
                let (mut writer, _, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let actor = tx.test_write_identity().0.unwrap();
                assert_eq!(actor, super::actor(1, 2, 2));
                assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, 6);
                live_equals_replay!(writer, owner);
                assert!(
                    writer
                        .abort_empty_transaction(owner, tx, private, &mut flush)
                        .is_ok()
                );
            }
            assert_eq!(live.catalog_test_locals(), (2, 2));
            // ADV-04: the Status is still occupied by the TerminalReady actor
            // of that abort, so the next Begin on it is WrongState before any
            // gate, source, local, WAL, file or bank change.
            let prefix = live.catalog_test_wal_state();
            let before_bank = live.recovery_test_bank().to_vec();
            let images = state.lock().unwrap().images();
            {
                let (mut writer, reader, mut flush) = live.split();
                let error = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        status,
                        private,
                        &mut flush,
                    )
                    .err()
                    .unwrap();
                assert!(matches!(error.cause, RetentionCause::WrongState));
                assert_eq!(error.phase, RetentionPhase::Prepare);
                assert_eq!(error.effect, Effect::Unchanged);
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert!(!hook::observe(core).read_gate_closed);
            }
            assert_eq!(live.catalog_test_locals(), (2, 2));
            assert_eq!(live.catalog_test_wal_state(), prefix);
            assert_eq!(live.recovery_test_bank(), before_bank);
            assert_eq!(live.catalog_test_isolation().1, 0);
            assert_eq!(state.lock().unwrap().images(), images);
            assert!(!fence.is_fenced());
            owner.check_idle().unwrap();
        });
    });
}

#[test]
fn row_begin_write_and_sync_failures_after_stage_are_fatal() {
    case("R1D-R03", || {
        for sync in [false, true] {
            let oracle = Oracle::new();
            with_fresh!(oracle, |live,
                                 owner,
                                 flusher,
                                 status,
                                 undo,
                                 private,
                                 reader_private,
                                 core,
                                 fence,
                                 state| {
                let _ = (reader_private, core);
                prepare_create(live, owner, flusher, status, undo);
                let start = state.lock().unwrap().files["d3-wal-00000001"].bytes.len();
                {
                    let mut adapter = state.lock().unwrap();
                    adapter.events.clear();
                    adapter.fault_hits = 0;
                    adapter.fault = Some(port::Fault {
                        key: port::Key::new(
                            if sync { "sync" } else { "write" },
                            "d3-wal-00000001",
                            if sync { 0 } else { start as u64 },
                            1,
                        ),
                        action: if sync {
                            port::Action::AfterError
                        } else {
                            port::Action::BeforeError
                        },
                    });
                }
                {
                    let (mut writer, _, mut flush) = live.split();
                    let error = writer
                        .begin_row_transaction(
                            owner,
                            IsolationLevel::Serializable,
                            status,
                            private,
                            &mut flush,
                        )
                        .err()
                        .unwrap();
                    assert_eq!(error.effect, Effect::CallerMustResolve);
                }
                assert!(fence.is_fenced());
                let (prefix, first_write) = live.catalog_test_wal_state();
                assert_eq!(
                    (prefix.complete_lsn, prefix.synced_lsn, first_write),
                    (6, 5, 6)
                );
                let adapter = state.lock().unwrap();
                assert_eq!(adapter.fault_hits, 1);
                assert!(adapter.fault.is_none());
            });
        }
    });
}

#[test]
fn row_capsule_refused_by_catalog_entries_and_aborts_empty() {
    case("R1D-R07", || {
        let oracle = Oracle::new();
        with_fresh!(oracle, |live,
                             owner,
                             flusher,
                             status,
                             undo,
                             private,
                             reader_private,
                             core,
                             fence,
                             state| {
            let _ = reader_private;
            prepare_create(live, owner, flusher, status, undo);
            let (mut writer, reader, mut flush) = live.split();
            let tx = writer
                .begin_row_transaction(
                    owner,
                    IsolationLevel::Serializable,
                    status,
                    private,
                    &mut flush,
                )
                .unwrap();
            let identity = tx.test_write_identity();
            let before = state.lock().unwrap().images();
            // Catalog entries refuse kind 2 before any gate, source or scan.
            // begin_statement/finish_statement dispatch by kind instead
            // (ROW-BEGIN-001); the Row statement has its own cases.
            let tx = match writer.commit_catalog_transaction(owner, tx, private) {
                Err(TransactionFinishError::Before { error, transaction }) => {
                    assert!(matches!(error.cause, RetentionCause::Unsupported));
                    assert_eq!(error.effect, Effect::Unchanged);
                    transaction
                }
                _ => panic!("Row capsule must return from Catalog Commit"),
            };
            let tx = match writer.abort_catalog_transaction(owner, tx, private, &mut flush) {
                Err(TransactionFinishError::Before { error, transaction }) => {
                    assert!(matches!(error.cause, RetentionCause::Unsupported));
                    assert_eq!(error.effect, Effect::Unchanged);
                    transaction
                }
                _ => panic!("Row capsule must return from Catalog abort"),
            };
            let tx = match writer.finish_read_only(owner, tx) {
                Err(TransactionFinishError::Before { error, transaction }) => {
                    assert!(matches!(error.cause, RetentionCause::WrongState));
                    transaction
                }
                _ => panic!("writable must not finish as read-only"),
            };
            assert_eq!(tx.test_write_identity(), identity);
            assert_eq!(state.lock().unwrap().images(), before);
            assert!(!hook::observe(core).read_gate_closed);
            assert_eq!(reader.catalog_test_source_used(), 0);
            // R08: the empty Row abort consumes the capsule (RollbackStart
            // LSN 7, AbortEnd LSN 8) with the Row Terminal TerminalReady.
            assert!(
                writer
                    .abort_empty_transaction(owner, tx, private, &mut flush)
                    .is_ok()
            );
            assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, 8);
            assert!(
                writer.recovery_test_bank()[..32 * 1664]
                    .iter()
                    .all(|&b| b == 0)
            );
            let actor = identity.0.unwrap();
            assert_eq!(
                row_job(writer.catalog_test_jobs(), actor),
                (0, JobState::TerminalReady, 2, 0)
            );
            live_equals_replay!(writer, owner);
            assert_eq!(reader.catalog_test_references().0.0, 0);
            assert_eq!(writer.catalog_test_source_used(), 0);
            assert!(!fence.is_fenced());
        });
    });
}

#[test]
fn row_abort_capacity_after_rollback_returns_same_must_abort_capsule() {
    case("R1D-R08", || {
        let oracle = Oracle::new();
        with_fresh!(oracle, |live,
                             owner,
                             flusher,
                             status,
                             undo,
                             private,
                             reader_private,
                             core,
                             fence,
                             state| {
            let _ = (reader_private, state);
            prepare_create(live, owner, flusher, status, undo);
            let (mut writer, reader, mut flush) = live.split();
            let tx = writer
                .begin_row_transaction(
                    owner,
                    IsolationLevel::Serializable,
                    status,
                    private,
                    &mut flush,
                )
                .unwrap();
            let identity = tx.test_write_identity();
            let (entered, arrived) = sync_channel(1);
            let (resume, release) = sync_channel(1);
            let (done, finished) = sync_channel(1);
            hook::install(core, Point::EmptyRollbackCompleted, entered, release);
            let held = 65536
                - writer.catalog_test_commit_charges()[0]
                - writer.catalog_test_retirement_charge()
                + 1;
            let reader_ref = &reader;
            let tx = std::thread::scope(|scope| {
                let controller = scope.spawn(move || {
                    arrived.recv_timeout(WAIT).unwrap();
                    assert_eq!(reader_ref.catalog_test_source_used(), 0);
                    reader_ref.catalog_test_with_held_source(held, || {
                        resume.send(Resume::Continue).unwrap();
                        finished.recv_timeout(WAIT).unwrap();
                    });
                });
                let tx = match writer.abort_empty_transaction(owner, tx, private, &mut flush) {
                    Err(TransactionFinishError::RollbackRequired { error, transaction }) => {
                        assert!(matches!(error.cause, RetentionCause::Capacity));
                        assert_eq!(error.effect, Effect::Unchanged);
                        transaction
                    }
                    _ => panic!("must return original unfinished Row abort"),
                };
                done.send(()).unwrap();
                controller.join().unwrap();
                tx
            });
            assert_eq!(
                tx.test_write_identity(),
                (identity.0, identity.1, identity.2, 2, 512)
            );
            assert_eq!(tx.test_write_kind(), (2, 2));
            assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, 7);
            let actor = identity.0.unwrap();
            assert_eq!(
                row_job(writer.catalog_test_jobs(), actor),
                (0, JobState::TerminalLeased, 0, 0)
            );
            live_equals_replay!(writer, owner);
            assert!(writer.begin_statement(owner, &tx).is_err());
            // Retry continues with AbortEnd only: no second RollbackStart.
            assert!(
                writer
                    .abort_empty_transaction(owner, tx, private, &mut flush)
                    .is_ok()
            );
            assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, 8);
            assert_eq!(
                row_job(writer.catalog_test_jobs(), actor),
                (0, JobState::TerminalReady, 2, 0)
            );
            live_equals_replay!(writer, owner);
            assert_eq!(reader.catalog_test_references().0.0, 0);
            assert_eq!(live.catalog_test_isolation(), (2, 0, 0));
            assert!(!fence.is_fenced());
        });
    });
}

/// A3: a mixed bank with one Row actor and one Catalog TerminalLeased actor.
/// The Catalog Begin/CREATE/Commit runs over it, the replay of the whole WAL
/// (Catalog groups through the Catalog arm, Row groups through the Row arm)
/// equals live after every step, and `check_ledger` plus the S1-c Catalog
/// Terminal scan run over the mixed bank. MBC40-V02: an actor of the other
/// scope sent to either lifecycle ingress is refused with stage 0.
#[test]
fn mixed_bank_catalog_commit_beside_row_actor_replays_equal() {
    case("R1D-R09", || {
        let oracle = Oracle::new();
        // One more frame for the second Status beside the CREATE pair.
        let images = with_fresh!(
            oracle,
            frames = 10,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = reader_private;
                let row_status = second_status(live, owner, flusher);
                prepare_create(live, owner, flusher, status, undo);
                {
                    let (mut writer, reader, mut flush) = live.split();
                    let row = writer
                        .begin_row_transaction(
                            owner,
                            IsolationLevel::Serializable,
                            &row_status,
                            private,
                            &mut flush,
                        )
                        .unwrap();
                    let row_actor = row.test_write_identity().0.unwrap();
                    assert_eq!(row.test_write_kind(), (2, 2));
                    assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, 8);
                    let tx = writer
                        .begin_catalog_transaction(
                            owner,
                            IsolationLevel::Serializable,
                            status,
                            private,
                            &mut flush,
                        )
                        .unwrap();
                    let catalog_actor = tx.test_write_identity().0.unwrap();
                    assert_eq!(tx.test_write_kind(), (1, 1));
                    assert_eq!(tx.test_write_identity().1, 1);
                    assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, 9);
                    assert_eq!(
                        row_job(writer.catalog_test_jobs(), row_actor),
                        (0, JobState::TerminalLeased, 0, 0)
                    );
                    // Mixed bank: Catalog TerminalLeased beside the Row actor.
                    let (catalog, skipped, ledger) =
                        writer.row_test_mixed_scans(catalog_actor, row_actor);
                    assert_eq!(catalog.unwrap(), 1);
                    let skipped = skipped.err().unwrap();
                    assert!(matches!(skipped.cause, RetentionCause::MissingTransaction));
                    assert_eq!(skipped.effect, Effect::Unchanged);
                    ledger.unwrap();
                    live_equals_replay!(writer, owner);
                    // MBC40-V02: scope-mismatched ingress has stage 0.
                    let before = state.lock().unwrap().images();
                    for result in [
                        writer
                            .row_rollback_start(owner, catalog_actor, private, &mut flush)
                            .map(|_| ()),
                        writer
                            .row_abort_end(owner, catalog_actor, private, &mut flush)
                            .map(|_| ()),
                        writer
                            .catalog_rollback_start(owner, row_actor, private, &mut flush)
                            .map(|_| ()),
                        writer
                            .catalog_abort_end(owner, row_actor, private, &mut flush)
                            .map(|_| ()),
                    ] {
                        let error = result.err().unwrap();
                        assert!(matches!(error.cause, RetentionCause::MissingTransaction));
                        assert_eq!(error.effect, Effect::Unchanged);
                    }
                    assert_eq!(state.lock().unwrap().images(), before);
                    assert_eq!(writer.catalog_test_source_used(), 0);
                    assert!(!hook::observe(core).read_gate_closed);
                    // Catalog Begin/CREATE/Commit over the mixed bank.
                    let view = writer.begin_statement(owner, &tx).unwrap();
                    assert_eq!(
                        writer
                            .catalog_test_forward_in_statement(
                                owner,
                                &view,
                                undo,
                                &table_spec("T"),
                                private,
                                &mut flush
                            )
                            .unwrap()
                            .object
                            .get(),
                        6
                    );
                    match writer.finish_statement(owner, view) {
                        Ok(()) => {}
                        Err(StatementFinishError::Before { error, .. }) => {
                            panic!("statement finish {error:?}")
                        }
                        Err(_) => panic!("statement finish"),
                    }
                    let commit = match writer.commit_catalog_transaction(owner, tx, private) {
                        Ok(commit) => commit,
                        Err(
                            TransactionFinishError::Before { error, .. }
                            | TransactionFinishError::RollbackRequired { error, .. }
                            | TransactionFinishError::Rejected(error)
                            | TransactionFinishError::Fatal(error),
                        ) => panic!("Catalog Commit beside a Row actor: {error:?}"),
                    };
                    assert_eq!(commit.q(), 1);
                    assert_eq!(
                        row_job(writer.catalog_test_jobs(), row_actor),
                        (0, JobState::TerminalLeased, 0, 0)
                    );
                    let (catalog, skipped, ledger) =
                        writer.row_test_mixed_scans(catalog_actor, row_actor);
                    assert_eq!(catalog.unwrap(), 1);
                    assert!(matches!(
                        skipped.err().unwrap().cause,
                        RetentionCause::MissingTransaction
                    ));
                    ledger.unwrap();
                    live_equals_replay!(writer, owner);
                    // The Row actor still aborts empty over the committed Catalog.
                    assert!(
                        writer
                            .abort_empty_transaction(owner, row, private, &mut flush)
                            .is_ok()
                    );
                    assert_eq!(
                        row_job(writer.catalog_test_jobs(), row_actor),
                        (0, JobState::TerminalReady, 2, 0)
                    );
                    live_equals_replay!(writer, owner);
                    assert_eq!(reader.catalog_test_references().0.0, 0);
                    assert_eq!(writer.catalog_test_source_used(), 0);
                }
                assert_eq!(live.catalog_test_q(), 1);
                assert_eq!(live.catalog_test_isolation().1, 0);
                query(live, owner, flusher, private, 1, 6, true);
                assert!(!fence.is_fenced());
                state.lock().unwrap().images()
            }
        );
        gated(&images);
    });
}

/// MBC10-V07 for the Row capsule: the long 512-byte class with 511, 512 and
/// 513 bytes remaining. The rollback and terminal of the empty abort enter
/// without a new foreground credit while the class stays occupied.
#[test]
fn row_begin_long_class_boundary() {
    case("R1D-V07", || {
        for remaining in [511_u64, 512, 513] {
            let oracle = Oracle::new();
            with_fresh!(oracle, |live,
                                 owner,
                                 flusher,
                                 status,
                                 undo,
                                 private,
                                 reader_private,
                                 core,
                                 fence,
                                 state| {
                let _ = (reader_private, core);
                prepare_create(live, owner, flusher, status, undo);
                let before = state.lock().unwrap().images();
                let held = 49_152 - remaining;
                {
                    let (mut writer, reader, mut flush) = live.split();
                    reader.catalog_test_with_held_long(held, || {
                        let result = writer.begin_row_transaction(
                            owner,
                            IsolationLevel::Serializable,
                            status,
                            private,
                            &mut flush,
                        );
                        if remaining < 512 {
                            let error = result.err().unwrap();
                            assert!(matches!(error.cause, RetentionCause::Capacity));
                            assert_eq!(error.effect, Effect::Unchanged);
                            assert_eq!(state.lock().unwrap().images(), before);
                        } else {
                            let tx = result.unwrap();
                            assert_eq!(tx.test_write_identity().4, held + 512);
                            assert!(
                                writer
                                    .abort_empty_transaction(owner, tx, private, &mut flush)
                                    .is_ok()
                            );
                            assert_eq!(writer.free_undo_test_state().unwrap().0.complete_lsn, 8);
                        }
                    });
                }
                assert_eq!(live.catalog_test_isolation().1, 0);
                assert_eq!(live.catalog_test_isolation().2, 0);
                let expected = if remaining < 512 { (0, 0) } else { (1, 1) };
                assert_eq!(live.catalog_test_locals(), expected);
                assert!(!fence.is_fenced());
            });
        }
    });
}

/// MBC10-V10 for Row progress: two foreground reads in progress, Row Begin
/// requests the read gate, a new reader is refused, both reads return.
#[test]
fn row_begin_waits_for_two_reads_and_refuses_new_reader() {
    case("R1D-V10", || {
        let oracle = Oracle::new();
        with_fresh!(oracle, |live,
                             owner,
                             flusher,
                             status,
                             undo,
                             private,
                             reader_private,
                             core,
                             fence,
                             state| {
            prepare_create(live, owner, flusher, status, undo);
            prepare_catalog(live, owner, flusher);
            let before = state.lock().unwrap().images();
            let mut second_private = boxed::<PRIVATE_BYTES>();
            let mut late_private = boxed::<PRIVATE_BYTES>();
            {
                let (mut writer, reader, mut flush) = live.split();
                let reader_ref = &reader;
                std::thread::scope(|scope| {
                    let mut readers = Vec::new();
                    let mut releases = Vec::new();
                    for private in [reader_private, &mut *second_private] {
                        let (entered, captured) = sync_channel(1);
                        let (resume, release) = sync_channel(1);
                        hook::install(core, Point::LookupCaptured, entered, release);
                        readers.push(scope.spawn(move || {
                            let mut active = core.try_owner(OwnerRole::Foreground).unwrap();
                            let snapshot = reader_ref.catalog_snapshot(&mut active).unwrap();
                            // The fixed resident class admits both readers.
                            let step = reader_ref
                                .lookup_catalog_resident(
                                    &mut active,
                                    &snapshot,
                                    ObjectId::new(6).unwrap(),
                                    &mut Plan4ResidencyCursor::new(),
                                    private,
                                )
                                .unwrap();
                            assert!(matches!(step, CatalogLookupStep::Missing));
                            snapshot.release_checked().unwrap();
                            active.check_idle().unwrap();
                        }));
                        captured.recv_timeout(WAIT).unwrap();
                        releases.push(resume);
                    }
                    assert_eq!(hook::observe(core).active_read_operations, 2);
                    let (entered, waiting) = sync_channel(1);
                    let (resume_wait, release) = sync_channel(1);
                    hook::install(core, Point::ReadGateWaiting, entered, release);
                    let late_private = &mut *late_private;
                    let controlling = scope.spawn(move || {
                        waiting.recv_timeout(WAIT).unwrap();
                        assert!(hook::observe(core).read_gate_closed);
                        assert_eq!(hook::observe(core).active_read_operations, 2);
                        assert_eq!(state.lock().unwrap().images(), before);
                        let mut late = core.try_owner(OwnerRole::Foreground).unwrap();
                        let snapshot = reader_ref.catalog_snapshot(&mut late).unwrap();
                        let error = reader_ref
                            .lookup_catalog_resident(
                                &mut late,
                                &snapshot,
                                ObjectId::new(6).unwrap(),
                                &mut Plan4ResidencyCursor::new(),
                                late_private,
                            )
                            .err()
                            .unwrap();
                        assert_eq!(error.effect, Effect::Unchanged);
                        assert!(matches!(error.cause, RetentionCause::Concurrency(error)
                            if error.cause == crate::transaction::concurrency::Cause::InstallationPending));
                        snapshot.release_checked().unwrap();
                        late.check_idle().unwrap();
                        resume_wait.send(Resume::Continue).unwrap();
                        for resume in releases {
                            resume.send(Resume::Continue).unwrap();
                        }
                        for reading in readers {
                            reading.join().unwrap();
                        }
                    });
                    let tx = writer
                        .begin_row_transaction(
                            owner,
                            IsolationLevel::Serializable,
                            status,
                            private,
                            &mut flush,
                        )
                        .unwrap();
                    controlling.join().unwrap();
                    assert_eq!(hook::observe(core).active_read_operations, 0);
                    assert!(!hook::observe(core).read_gate_closed);
                    assert_eq!(reader.catalog_test_source_used(), 0);
                    assert!(
                        writer
                            .abort_empty_transaction(owner, tx, private, &mut flush)
                            .is_ok()
                    );
                });
            }
            assert_eq!(live.catalog_test_isolation(), (2, 0, 0));
            assert!(!fence.is_fenced());
            owner.check_idle().unwrap();
        });
    });
}

/// A5 negative: the live/replay comparison is real. One flipped byte of the
/// live Row actor's ATT row makes the next `history::validate_live` (and so
/// the next Row lifecycle preparation) a fenced Fatal with no WAL effect.
#[test]
fn row_live_replay_mismatch_is_fatal() {
    case("R1D-A5N", || {
        let oracle = Oracle::new();
        with_fresh!(oracle, |live,
                             owner,
                             flusher,
                             status,
                             undo,
                             private,
                             reader_private,
                             core,
                             fence,
                             state| {
            let _ = (reader_private, core);
            prepare_create(live, owner, flusher, status, undo);
            let (mut writer, _, mut flush) = live.split();
            let tx = writer
                .begin_row_transaction(
                    owner,
                    IsolationLevel::Serializable,
                    status,
                    private,
                    &mut flush,
                )
                .unwrap();
            let actor = tx.test_write_identity().0.unwrap();
            live_equals_replay!(writer, owner);
            let before = state.lock().unwrap().images();
            writer.row_test_flip_control(ATT_ACTOR.start + 40);
            let (checked, copies) = writer.row_test_live_replay(owner);
            let error = checked.err().unwrap();
            assert_eq!(error.effect, Effect::CallerMustResolve);
            assert!(matches!(error.cause, RetentionCause::Invariant));
            assert_ne!(copies[0], copies[2]);
            assert!(fence.is_fenced());
            let error = writer
                .row_rollback_start(owner, actor, private, &mut flush)
                .err()
                .unwrap();
            assert_eq!(error.effect, Effect::CallerMustResolve);
            assert_eq!(state.lock().unwrap().images(), before);
            drop(tx);
        });
    });
}
