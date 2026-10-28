//! Actual Q publication gaps. A reader of the original visibility binding
//! registers inside the gap; hooks only pause the original Commit producer.
use super::*;
use crate::transaction::concurrency::catalog_test_support::{self as hook, Point, Resume};
use std::sync::mpsc::sync_channel;

fn id(base: usize, offset: usize) -> String {
    format!("C{:02}", base + offset)
}

/// Begin/Reserve/Forward of one same-epoch CREATE from the frozen C corpus.
#[allow(
    clippy::too_many_arguments,
    reason = "actual owners and disjoint loans remain explicit"
)]
fn forward<'s, 'c>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, port::Writer>,
    owner: &mut Owner<'c, 's>,
    flusher: &mut Owner<'c, 's>,
    status: &Plan4Target,
    undo: &Plan4Target,
    private: &mut [u8; PRIVATE_BYTES],
    oracle: &Oracle,
    state: &State,
    base: usize,
) -> TxnRef {
    prepare_create(live, owner, flusher, status, undo);
    let actor = oracle.actor(&id(base, 1));
    let begin = live.catalog_begin(owner, status, private).unwrap();
    assert_eq!(
        (begin.header.lsn, begin.actor),
        (oracle.row(&id(base, 1))[3].parse().unwrap(), Some(actor))
    );
    oracle.check(live, state, &id(base, 1));
    let object = if base == 0 { 6 } else { 7 };
    let definition = table_spec(if base == 0 { "T" } else { "U" });
    let reserved = live
        .reserve_catalog_create(owner, actor, undo, &definition, private)
        .unwrap();
    assert_eq!(reserved.object().object.get(), object);
    oracle.check(reserved.catalog_test_bound(), state, &id(base, 2));
    match reserved.initialize() {
        Ok(created) => assert_eq!(created.into_parts().0.object.get(), object),
        Err(RetentionReservationError::Before { error, .. })
        | Err(RetentionReservationError::Fatal(error)) => panic!("actual Forward {error:?}"),
    }
    oracle.check(live, state, &id(base, 3));
    actor
}

/// Prune, three ReleaseRole, RootFree and TerminalFinish of the same cycle.
fn cleanup<'s, 'c>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, port::Writer>,
    owner: &mut Owner<'c, 's>,
    private: &mut [u8; PRIVATE_BYTES],
    oracle: &Oracle,
    state: &State,
    base: usize,
) {
    let at = |offset| id(base, offset);
    retirement_prepare::retire(
        live,
        owner,
        oracle.ordinal(&at(5)),
        Operation::Prune,
        private,
    );
    oracle.check(live, state, &at(5));
    for offset in 6..=8 {
        live.catalog_release_role(owner, oracle.ordinal(&at(offset)), private)
            .unwrap();
        oracle.check(live, state, &at(offset));
    }
    live.catalog_retire(owner, oracle.ordinal(&at(9)), Operation::RootFree, private)
        .unwrap();
    oracle.check(live, state, &at(9));
    live.catalog_retire(
        owner,
        oracle.ordinal(&at(10)),
        Operation::TerminalFinish,
        private,
    )
    .unwrap();
    oracle.check(live, state, &at(10));
}

fn visibility_gap(case_id: &str, base: usize) {
    case(case_id, || {
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
            if base == 10 {
                let actor = forward(
                    live, owner, flusher, status, undo, private, &oracle, state, 0,
                );
                let prepared = live.prepare_catalog_commit(owner, actor, private).unwrap();
                live.split().0.commit_catalog(owner, prepared).unwrap();
                oracle.check(live, state, "C04");
                cleanup(live, owner, private, &oracle, state, 0);
            }
            let epoch_backing = live.catalog_test_epoch_backing();
            assert!(epoch_backing.1);
            let before_q = (base / 10) as u64;
            let actor = forward(
                live, owner, flusher, status, undo, private, &oracle, state, base,
            );
            let prepared = live.prepare_catalog_commit(owner, actor, private).unwrap();
            let (entered, arrived) = sync_channel(1);
            let (resume, release) = sync_channel(1);
            let (lend, lent) = sync_channel(1);
            let (drop_old, dropping) = sync_channel(1);
            hook::install(core, Point::BeforePublish, entered, release);
            // The writer's private loan stays bound to the Commit receipt. The
            // reader lends back its disjoint loan while its snapshot remains.
            let reader_private = {
                let (mut writer, reader, mut flush) = live.split();
                std::thread::scope(|scope| {
                    let core_ref = core;
                    let reading = scope.spawn(move || {
                        arrived.recv_timeout(WAIT).unwrap();
                        let mut gap = core_ref.try_owner(OwnerRole::Foreground).unwrap();
                        let snapshot = reader.catalog_snapshot(&mut gap).unwrap();
                        assert_eq!(snapshot.q(), before_q);
                        assert_eq!(reader.catalog_test_horizon(), (before_q, before_q));
                        assert!(!lookup_resident!(
                            reader,
                            &mut gap,
                            &snapshot,
                            6 + before_q,
                            reader_private
                        ));
                        if base == 10 {
                            assert!(lookup_resident!(
                                reader,
                                &mut gap,
                                &snapshot,
                                6,
                                reader_private
                            ));
                        }
                        let observation = hook::observe(core_ref);
                        assert_eq!(
                            (observation.active_apply, observation.active_content),
                            (0, 0)
                        );
                        assert_eq!(observation.installation, "Open");
                        resume.send(Resume::Continue).unwrap();
                        lend.send(reader_private).unwrap();
                        dropping.recv_timeout(WAIT).unwrap();
                        assert_eq!(reader.catalog_test_horizon(), (before_q + 1, before_q));
                        snapshot.release_checked().unwrap();
                        assert_eq!(reader.catalog_test_horizon(), (before_q + 1, before_q + 1));
                        gap.check_idle().unwrap();
                    });
                    writer.commit_catalog(owner, prepared).unwrap();
                    let spare = lent.recv_timeout(WAIT).unwrap();
                    let before = state.lock().unwrap().images();
                    let ordinal = oracle.ordinal(&id(base, 5));
                    let error = writer
                        .retire_catalog(owner, ordinal, Operation::Prune, spare, &mut flush)
                        .err()
                        .expect("old snapshot must keep Prune below committed q");
                    assert!(
                        busy(error.effect, &error.cause),
                        "Prune with old snapshot: {error:?}"
                    );
                    assert!(!fence.is_fenced());
                    assert_eq!(state.lock().unwrap().images(), before, "Busy Prune WAL0");
                    owner.check_idle().unwrap();
                    drop_old.send(()).unwrap();
                    reading.join().unwrap();
                    spare
                })
            };
            oracle.check(live, state, &id(base, 4));
            assert_eq!(live.catalog_test_q(), before_q + 1);
            let expected_epochs = if base == 0 {
                vec![(2, 6, 1)]
            } else {
                vec![(2, 6, 1), (2, 7, 2)]
            };
            assert_eq!(
                live.catalog_test_epochs(),
                (true, before_q + 1, expected_epochs.clone())
            );
            cleanup(live, owner, private, &oracle, state, base);
            query(live, owner, flusher, reader_private, before_q + 1, 6, true);
            if base == 10 {
                query(live, owner, flusher, reader_private, 2, 7, true);
            }
            assert!(!fence.is_fenced());
            // Q and the actual CREATE epochs publish together; physical cleanup
            // does not advance the table or namespace epoch.
            assert_eq!(live.catalog_test_epoch_backing(), epoch_backing);
            assert_eq!(
                live.catalog_test_epochs(),
                (true, before_q + 1, expected_epochs)
            );
            owner.check_idle().unwrap();
            flusher.check_idle().unwrap();
        });
    });
}

#[test]
fn catalog_mt01_q0_publication_gap() {
    visibility_gap("MT01", 0);
}
#[test]
fn catalog_mt02_q1_publication_gap() {
    visibility_gap("MT02", 10);
}
