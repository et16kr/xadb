//! Actual physical Busy rejection after StageCallStarted. The competing stage
//! is a synthetic fault fixture, not support for a second semantic writer.
use super::*;
use crate::{
    storage::{
        disk::{RetentionCause, RetentionPhase},
        wal_io::{Cause, Phase},
    },
    transaction::concurrency::catalog_test_support::{self as hook, Point, Resume},
};
use std::{sync::mpsc::sync_channel, time::Duration};

#[test]
fn physical_busy_restores_only_the_rejected_stage() {
    case("C3_TERMINAL_BUSY", || {
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
            let actor = create_forward(
                live, owner, flusher, status, undo, private, &oracle, state, 1,
            );
            let before_files = state.lock().unwrap().images();
            let before_bank = live.recovery_test_bank().to_vec();
            let before_jobs = live.catalog_test_jobs().to_vec();
            let before_epochs = live.catalog_test_epochs();
            let before_refs = live.catalog_test_references();
            let before_wal = live.catalog_test_wal_state();
            let mut originals = Vec::new();
            for number in [0, 1, 2, 9] {
                let index = resident_index(live, number).unwrap();
                let mut bytes = boxed::<PAGE_BYTES>();
                let observed = probe::diagnose(live.catalog_test_pool(), index, &mut bytes);
                originals.push((index, observed, bytes));
            }
            let prepared = live.prepare_catalog_commit(owner, actor, private).unwrap();
            let bytes = prepared.frame().to_vec();
            assert_eq!(bytes, oracle.frame("C04"));
            let (entered, arrived) = sync_channel(1);
            let (resume, release) = sync_channel(1);
            hook::install(core, Point::StageCallStarted, entered, release);
            state.lock().unwrap().events.clear();
            let error = {
                let (mut writer, reader, _) = live.split();
                std::thread::scope(|scope| {
                    let controller = scope.spawn(move || {
                        arrived.recv_timeout(Duration::from_secs(5)).unwrap();
                        reader.catalog_test_competing_stage(&bytes);
                        resume.send(Resume::Continue).unwrap();
                    });
                    let error = writer
                        .commit_catalog(owner, prepared)
                        .err()
                        .expect("actual Busy");
                    controller.join().unwrap();
                    error
                })
            };
            assert_eq!(
                (error.phase, error.effect),
                (RetentionPhase::Stage, Effect::Unchanged)
            );
            assert!(matches!(error.cause, RetentionCause::Wal(error)
                if error.phase == Phase::Stage && matches!(error.cause, Cause::Busy)
                   && error.effect == Effect::Unchanged));
            assert!(
                !fence.is_fenced(),
                "the actual rejected call must not fence"
            );
            owner.check_idle().unwrap();
            let observed = hook::observe(core);
            assert_eq!((observed.active_apply, observed.active_content), (0, 0));
            assert_eq!(observed.installation, "Open");
            assert_eq!(live.catalog_test_q(), 0);
            assert_eq!(live.recovery_test_bank(), before_bank);
            assert_eq!(live.catalog_test_jobs().as_slice(), before_jobs);
            assert_eq!(live.catalog_test_epochs(), before_epochs);
            assert_eq!(live.catalog_test_references(), before_refs);
            assert_eq!(state.lock().unwrap().images(), before_files);
            assert!(state.lock().unwrap().events.is_empty());
            for (index, original, bytes) in originals {
                let mut after = boxed::<PAGE_BYTES>();
                let actual = probe::diagnose(live.catalog_test_pool(), index, &mut after);
                assert_eq!(after, bytes);
                assert_eq!(
                    (
                        actual.frame,
                        actual.dirty,
                        actual.entry,
                        actual.pins,
                        actual.total
                    ),
                    (
                        original.frame,
                        original.dirty,
                        original.entry,
                        original.pins,
                        original.total
                    )
                );
            }
            let (prefix, first_write) = live.catalog_test_wal_state();
            assert_eq!((prefix.complete_lsn, prefix.synced_lsn), (9, 8));
            assert_eq!(first_write, before_wal.1);
            assert_eq!(
                prefix.complete_offset,
                before_wal.0.complete_offset + oracle.frame("C04").len() as u64
            );
            assert_eq!(prefix.synced_offset, before_wal.0.synced_offset);
            // The competing complete frame is an outstanding physical obligation.
            // Do not erase it or reuse this session as a healthy semantic writer.
            fence.fence();
            assert!(fence.is_fenced());
        });
    });
}
