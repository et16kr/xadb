//! S7: the MBC10-V09 replacement oracle for the CatalogProgress writer (S2-0
//! Row write amendment §7, contract `#row-join-s7`). The RowWrite writer is
//! `row_v09.rs`; the RowProgress writer is S1E-V09. This file supplies the
//! third progress class with the same oracle, over the actual uncommitted
//! CREATE of table T (`create_forward`, R03) and its actual Catalog rollback
//! start (`catalog_rollback_start`, R04):
//! - two Catalog resident readers are admitted and paused after their
//!   capture;
//! - the rollback start closes the terminal read gate and waits at it
//!   (ReadGateWaiting) for both returns, holding no charge, while a new
//!   reader is Unchanged InstallationPending;
//! - after one return it still waits (no LifecyclePreparing within `STILL`);
//! - only after both readers return is `CatalogProgressCharge` admitted.
//!
//! The last admission is applied at three sizes of the share a test-only
//! held loan leaves (S2W-R02's helper, the consumer MCA05 precedent):
//! - one byte short is Unchanged (Census, Capacity), with the gate reopened,
//!   no reference, no pin and no file change; a retry without the loan then
//!   completes R04;
//! - exactly and one byte over are admitted at LifecyclePreparing with no
//!   read active and the gate still closed, and complete R04.
//!
//! No cap changes. A PASS does not prove that the writer and two resident
//! readers are admitted together: the gate makes them exclusive.
use super::*;
use crate::storage::disk::{RetentionCause, RetentionPhase};
use crate::transaction::concurrency::{
    self,
    catalog_test_support::{self as hook, Point, Resume},
};
use std::sync::mpsc::sync_channel;

/// An object no Catalog record has: T (object 6) is the uncommitted CREATE,
/// so a resident lookup of 7 captures its absence and returns Missing.
const ABSENT: u64 = 7;
/// How long the first return is left alone before the writer is checked (the
/// gate's wait rechecks every 10 ms; `row_v09.rs`).
const STILL: std::time::Duration = std::time::Duration::from_millis(200);

/// A resident lookup by a new Foreground owner while the read gate is closed
/// is refused before its source admission.
macro_rules! refused_reader {
    ($reader:expr, $core:expr, $private:expr) => {{
        let mut late = $core.try_owner(OwnerRole::Foreground).unwrap();
        let snapshot = $reader.catalog_snapshot(&mut late).unwrap();
        let error = $reader
            .lookup_catalog_resident(
                &mut late,
                &snapshot,
                ObjectId::new(ABSENT).unwrap(),
                &mut Plan4ResidencyCursor::new(),
                $private,
            )
            .err()
            .unwrap();
        snapshot.release_checked().unwrap();
        late.check_idle().unwrap();
        error
    }};
}

/// One fresh fixture per `over`: the rollback start consumes the actor's
/// CREATE, so the rounds cannot share it.
fn admission(over: i64) {
    case("MBC10-V09", || {
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
            let actor = create_forward(
                live, owner, flusher, status, undo, private, &oracle, state, 1,
            );
            let before = state.lock().unwrap().images();
            let mut second_private = boxed::<PRIVATE_BYTES>();
            let mut late_private = boxed::<PRIVATE_BYTES>();
            {
                let (mut writer, reader, mut flush) = live.split();
                let shared = writer.catalog_test_commit_charges()[0];
                let charge = writer.catalog_test_retirement_charge();
                let [reader_shared, _, resident] = reader.catalog_test_fixed_charges();
                assert_eq!(shared, reader_shared);
                let used = reader.catalog_test_source_used();
                // Two resident readers fit beside the held loan at every size;
                // the writer's charge alone decides the boundary.
                assert!(2 * resident < charge - 1);
                let held = ((65_536 - shared - charge) as i64 - used as i64 - over) as usize;
                let held_used = used + held as u64;
                println!(
                    "MBC10_V09 writer=CatalogProgress over={over} shared={shared} \
                     charge={charge} resident={resident} used={used} held={held} \
                     shared_plus_charge={} shared_plus_two_resident={}",
                    shared + charge,
                    shared + 2 * resident
                );
                let reader_ref = &reader;
                let expected = &before;
                let late_private = &mut *late_private;
                let result = reader.catalog_test_with_held_source(held, || {
                    std::thread::scope(|scope| {
                        let mut readers = Vec::new();
                        let mut releases = Vec::new();
                        for private in [&mut *reader_private, &mut *second_private] {
                            let (entered, captured) = sync_channel(1);
                            let (resume, release) = sync_channel(1);
                            hook::install(core, Point::LookupCaptured, entered, release);
                            readers.push(scope.spawn(move || {
                                // The !Send Owner stays on this thread.
                                let mut active = core.try_owner(OwnerRole::Foreground).unwrap();
                                let snapshot = reader_ref.catalog_snapshot(&mut active).unwrap();
                                let step = reader_ref
                                    .lookup_catalog_resident(
                                        &mut active,
                                        &snapshot,
                                        ObjectId::new(ABSENT).unwrap(),
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
                        let admitted = hook::observe(core);
                        assert_eq!(admitted.active_read_operations, 2);
                        assert!(!admitted.read_gate_closed);
                        assert_eq!(
                            reader.catalog_test_source_used(),
                            held_used + 2 * resident as u64
                        );
                        let (entered, waiting) = sync_channel(1);
                        let (resume_wait, release) = sync_channel(1);
                        hook::install(core, Point::ReadGateWaiting, entered, release);
                        let controlling = scope.spawn(move || {
                            waiting.recv_timeout(WAIT).unwrap();
                            // The writer waits at the closed gate holding no
                            // charge and no file effect; a new reader is refused.
                            let at_wait = hook::observe(core);
                            let source_at_wait = reader_ref.catalog_test_source_used();
                            let unchanged_at_wait = state.lock().unwrap().images() == *expected;
                            let late = refused_reader!(reader_ref, core, late_private);
                            let source_after_late = reader_ref.catalog_test_source_used();
                            let admission = (over >= 0).then(|| {
                                let (entered, preparing) = sync_channel(1);
                                let (resume, release) = sync_channel(1);
                                hook::install(core, Point::LifecyclePreparing, entered, release);
                                (preparing, resume)
                            });
                            resume_wait.send(Resume::Continue).unwrap();
                            // One return: the writer still waits at the gate.
                            let mut releases = releases.into_iter();
                            let mut readers = readers.into_iter();
                            releases.next().unwrap().send(Resume::Continue).unwrap();
                            readers.next().unwrap().join().unwrap();
                            match &admission {
                                Some((preparing, _)) => assert!(
                                    preparing.recv_timeout(STILL).is_err(),
                                    "admitted with a read still active"
                                ),
                                None => std::thread::sleep(STILL),
                            }
                            let after_one = hook::observe(core);
                            let source_after_one = reader_ref.catalog_test_source_used();
                            // The second return.
                            releases.next().unwrap().send(Resume::Continue).unwrap();
                            readers.next().unwrap().join().unwrap();
                            // Admitted only after both readers returned.
                            let admitted = admission.map(|(preparing, resume)| {
                                preparing.recv_timeout(WAIT).unwrap();
                                let observed = hook::observe(core);
                                let source = reader_ref.catalog_test_source_used();
                                let unchanged = state.lock().unwrap().images() == *expected;
                                resume.send(Resume::Continue).unwrap();
                                (observed, source, unchanged)
                            });
                            (
                                (at_wait, source_at_wait, unchanged_at_wait),
                                (late, source_after_late),
                                (after_one, source_after_one),
                                admitted,
                            )
                        });
                        let result =
                            writer.catalog_rollback_start(owner, actor, private, &mut flush);
                        let (wait, late, one, admitted) = controlling.join().unwrap();
                        let (
                            (at_wait, source_at_wait, unchanged_at_wait),
                            (late, source_after_late),
                        ) = (wait, late);
                        let (after_one, source_after_one) = one;
                        println!(
                            "MBC10_V09 writer=CatalogProgress over={over} at_wait={at_wait:?} \
                             source_at_wait={source_at_wait} late_reader={late:?} \
                             after_one={after_one:?} source_after_one={source_after_one} \
                             admitted={admitted:?}"
                        );
                        assert!(at_wait.read_gate_closed);
                        assert_eq!(at_wait.active_read_operations, 2);
                        assert!(!at_wait.fenced);
                        assert_eq!(source_at_wait, held_used + 2 * resident as u64);
                        assert!(unchanged_at_wait, "the gate wait has no file effect");
                        assert_eq!(late.effect, Effect::Unchanged);
                        assert!(matches!(late.cause, RetentionCause::Concurrency(error)
                            if error.cause == concurrency::Cause::InstallationPending));
                        assert_eq!(source_after_late, source_at_wait);
                        // After one return: still waiting, no writer charge.
                        assert!(after_one.read_gate_closed);
                        assert_eq!(after_one.active_read_operations, 1);
                        assert!(!after_one.fenced);
                        assert_eq!(source_after_one, held_used + resident as u64);
                        match &admitted {
                            None => assert_eq!(over, -1),
                            Some((observed, source, unchanged)) => {
                                assert!(over >= 0);
                                assert!(observed.read_gate_closed);
                                assert_eq!(observed.active_read_operations, 0);
                                assert!(!observed.fenced);
                                assert_eq!(*source, held_used + charge as u64);
                                assert_eq!(*source, (65_536 - shared) as u64 - over as u64);
                                assert!(*unchanged, "preparation has WAL0");
                            }
                        }
                        // Either way the gate is open and the loan alone is held.
                        assert!(!hook::observe(core).read_gate_closed);
                        assert_eq!(reader.catalog_test_source_used(), held_used);
                        result
                    })
                });
                assert_eq!(reader.catalog_test_source_used(), used);
                match result {
                    Err(error) => {
                        println!("MBC10_V09 writer=CatalogProgress over={over} refusal={error:?}");
                        assert_eq!(over, -1);
                        assert_eq!(
                            (error.phase, error.effect),
                            (RetentionPhase::Census, Effect::Unchanged)
                        );
                        assert!(matches!(error.cause, RetentionCause::Capacity));
                        assert_eq!(state.lock().unwrap().images(), before);
                        assert_eq!(writer.catalog_test_references(), ((0, 0, 0), false));
                        assert_eq!(writer.catalog_test_physical_pins(), 0);
                        assert!(!fence.is_fenced());
                        // The refusal left the CREATE intact: a retry without
                        // the loan completes the same rollback start.
                        writer
                            .catalog_rollback_start(owner, actor, private, &mut flush)
                            .unwrap();
                    }
                    Ok(_) => assert!(over >= 0),
                }
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(writer.catalog_test_references(), ((0, 0, 0), false));
                assert_eq!(writer.catalog_test_physical_pins(), 0);
            }
            oracle.check(live, state, "R04");
            assert!(!fence.is_fenced());
            owner.check_idle().unwrap();
            println!("MBC10_V09 writer=CatalogProgress over={over} R04=accepted");
        });
    });
}

#[test]
fn mbc10_v09_catalog_progress_minus_one() {
    admission(-1);
}

#[test]
fn mbc10_v09_catalog_progress_exact() {
    admission(0);
}

#[test]
fn mbc10_v09_catalog_progress_plus_one() {
    admission(1);
}
