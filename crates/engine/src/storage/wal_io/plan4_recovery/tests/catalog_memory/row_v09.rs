//! S2-f: the MBC10-V09 replacement oracle for the RowWrite writer (S2-0 Row
//! write amendment §7 and the 2026-10-29 amendment of the MBC table, which
//! retire the original nested-sum wording). It runs over the actual
//! committed CREATE of table T that the preflight, reserve and cancel cases
//! use, and follows S1E-V09 (`row_statements.rs`), the RowProgress
//! precedent:
//! - two Catalog resident readers are admitted and paused after their
//!   capture;
//! - the Row write preflight closes the terminal read gate and waits at it
//!   (ReadGateWaiting) for both returns, holding no charge, while a new
//!   reader is Unchanged InstallationPending;
//! - only after both readers return is the RowWrite charge admitted.
//!
//! That last admission is applied at three sizes of the share a test-only
//! held loan leaves (S2W-R02's helper, the consumer MCA05 precedent of
//! limiting what the test admission has left): one byte short, exactly the
//! charge, and one byte over.
//! - One byte short is Unchanged (Census, Capacity), with the gate reopened
//!   and no effect.
//! - Exactly and one byte over are Ready; the last one reserves and then
//!   cancels.
//!
//! No cap changes. A PASS does not prove that the writer and two resident
//! readers are admitted together: the gate makes them exclusive.
use super::row_preflight::create_table;
use super::*;
use crate::storage::{
    disk::{
        IsolationLevel, RetentionCause, RetentionPhase, RowOwnedError, RowPreflightStep, TableStep,
    },
    row::insert::validate_insert,
    row_record::ValueRef,
};
use crate::transaction::concurrency::{
    self,
    catalog_test_support::{self as hook, Point, Resume},
};
use std::sync::mpsc::sync_channel;

/// The one value of T's row (`i`, Int64, not null).
const VALUE: i64 = 7;
/// An object no Catalog record has (the fixture commits T as object 6): a
/// resident lookup of it captures its absence and returns Missing.
const ABSENT: u64 = 7;
/// The fixture's WAL complete LSN before the reserve: the RowReserve and its
/// Cancel are the next two LSNs (`row_cancel.rs`).
const PREVIOUS_LSN: u64 = 14;
/// How long the first return is left alone before the writer is checked. A
/// writer that waited for only one return would recheck the read set within
/// it (the gate's wait rechecks every 10 ms, `FENCE_RECHECK`) and then show
/// up: at RowPreflightResident, or with the gate reopened after its refusal.
const STILL: std::time::Duration = std::time::Duration::from_millis(200);

/// The WAL file's length in the directory: the ReservedRow borrows the
/// writer, so the reserve's effect is read from the files.
fn wal_bytes(state: &State) -> usize {
    state.lock().unwrap().files["d3-wal-00000001"].bytes.len()
}

/// The late reader: a resident lookup by a new Foreground owner while the
/// read gate is closed is refused before its source admission.
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

/// MBC10-V09 (replacement oracle, RowWrite). One round per `over`, each with
/// the last admission limited by a held test loan to the charge plus `over`.
///
/// Before the gate, two resident readers hold `2 × resident`. Then:
/// - at ReadGateWaiting, the gate is closed, both reads are active and the
///   writer holds no charge;
/// - a new reader is Unchanged InstallationPending and changes no charge.
///
/// The readers return one at a time. After the first, the writer still
/// waits: for `STILL` it reaches no RowPreflightResident, and then the gate
/// is closed, one read is active and the ledger holds the held loan and one
/// resident, with no writer charge. Only then is the second released.
///
/// After both readers return:
/// - `over = -1` is Unchanged (Census, Capacity), with the gate reopened,
///   nothing registered and no WAL or file effect;
/// - `over = 0` and `over = 1` admit the charge, with no read active and the
///   gate still closed, at RowPreflightResident.
///
/// `over = 0` fills the ledger to exactly `65536 − shared` and finishes.
/// `over = 1` reserves (the WAL grows) and cancels (the complete LSN is then
/// the fixture's plus the RowReserve and the Cancel). The cancel returns the
/// charge with no fence.
///
/// The equal round relies on a returning reader closing its source before
/// its read operation ends (`finish_checked`, source-before-progress in
/// `plan4_retention/read/scope.rs`). Otherwise the woken writer could still
/// see that reader's resident charge and be refused Capacity.
#[test]
fn mbc10_v09_row_write_waits_for_two_resident_readers_then_reserves() {
    case("MBC10-V09", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                view.test_clear_reads();
                let shared = writer.catalog_test_commit_charges()[0];
                let charge = writer.catalog_test_retirement_charge();
                let [reader_shared, _, resident] = reader.catalog_test_fixed_charges();
                assert_eq!(shared, reader_shared);
                let used = reader.catalog_test_source_used();
                // Two resident readers fit beside the held loan at every size
                // below; the writer's charge alone decides the boundary.
                assert!(2 * resident < charge - 1);
                println!(
                    "MBC10_V09 shared={shared} charge={charge} resident={resident} used={used} \
                     shared_plus_charge={} shared_plus_two_resident={}",
                    shared + charge,
                    shared + 2 * resident
                );
                let mut second_private = boxed::<PRIVATE_BYTES>();
                let mut late_private = boxed::<PRIVATE_BYTES>();
                for over in [-1_i64, 0, 1] {
                    let held = ((65_536 - shared - charge) as i64 - used as i64 - over) as usize;
                    let held_used = used + held as u64;
                    let before = effects!(writer, state);
                    let reader_ref = &reader;
                    let late_private = &mut *late_private;
                    reader.catalog_test_with_held_source(held, || {
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
                                    let snapshot =
                                        reader_ref.catalog_snapshot(&mut active).unwrap();
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
                                // The writer waits at the closed gate holding
                                // no charge; a new reader is refused.
                                let at_wait = hook::observe(core);
                                let source_at_wait = reader_ref.catalog_test_source_used();
                                let late = refused_reader!(reader_ref, core, late_private);
                                let source_after_late = reader_ref.catalog_test_source_used();
                                let admission = (over >= 0).then(|| {
                                    let (entered, checking) = sync_channel(1);
                                    let (resume, release) = sync_channel(1);
                                    hook::install(
                                        core,
                                        Point::RowPreflightResident,
                                        entered,
                                        release,
                                    );
                                    (checking, resume)
                                });
                                resume_wait.send(Resume::Continue).unwrap();
                                // One return: the writer still waits at the gate.
                                let mut releases = releases.into_iter();
                                let mut readers = readers.into_iter();
                                releases.next().unwrap().send(Resume::Continue).unwrap();
                                readers.next().unwrap().join().unwrap();
                                match &admission {
                                    Some((checking, _)) => assert!(
                                        checking.recv_timeout(STILL).is_err(),
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
                                let admitted = admission.map(|(checking, resume)| {
                                    checking.recv_timeout(WAIT).unwrap();
                                    let observed = hook::observe(core);
                                    let source = reader_ref.catalog_test_source_used();
                                    resume.send(Resume::Continue).unwrap();
                                    (observed, source)
                                });
                                let one = (after_one, source_after_one);
                                (
                                    at_wait,
                                    source_at_wait,
                                    late,
                                    source_after_late,
                                    one,
                                    admitted,
                                )
                            });
                            let result =
                                writer.preflight_row(owner, &view, &table, private, &mut flush);
                            let (at_wait, source_at_wait, late, source_after_late, one, admitted) =
                                controlling.join().unwrap();
                            let (after_one, source_after_one) = one;
                            println!(
                                "MBC10_V09 over={over} held={held} at_wait={at_wait:?} \
                                 source_at_wait={source_at_wait} late_reader={late:?} \
                                 after_one={after_one:?} source_after_one={source_after_one} \
                                 admitted={admitted:?}"
                            );
                            assert!(at_wait.read_gate_closed);
                            assert_eq!(at_wait.active_read_operations, 2);
                            assert!(!at_wait.fenced);
                            assert_eq!(source_at_wait, held_used + 2 * resident as u64);
                            assert_eq!(late.effect, Effect::Unchanged);
                            assert!(matches!(late.cause, RetentionCause::Concurrency(error)
                                if error.cause == concurrency::Cause::InstallationPending));
                            assert_eq!(source_after_late, source_at_wait);
                            // After one return: still waiting, no writer charge.
                            assert!(after_one.read_gate_closed);
                            assert_eq!(after_one.active_read_operations, 1);
                            assert!(!after_one.fenced);
                            assert_eq!(source_after_one, held_used + resident as u64);
                            match result {
                                Err(error) => {
                                    println!("MBC10_V09 over={over} refusal={error:?}");
                                    assert_eq!(over, -1);
                                    assert!(admitted.is_none());
                                    assert_eq!(error.phase, RetentionPhase::Census);
                                    assert_eq!(error.effect, Effect::Unchanged);
                                    assert!(matches!(error.cause, RetentionCause::Capacity));
                                    returned_unchanged!(
                                        writer, reader, view, state, core, fence, held_used, before
                                    );
                                }
                                Ok(RowPreflightStep::Ready(mut preflight)) => {
                                    assert!(over >= 0);
                                    let (observed, source) = admitted.unwrap();
                                    assert!(observed.read_gate_closed);
                                    assert_eq!(observed.active_read_operations, 0);
                                    assert_eq!(source, held_used + charge as u64);
                                    assert_eq!(source, (65_536 - shared) as u64 - over as u64);
                                    assert_eq!(reader.catalog_test_source_used(), source);
                                    if over == 0 {
                                        preflight.finish().unwrap();
                                        assert_eq!(effects!(writer, state), before);
                                    } else {
                                        let input = validate_insert(
                                            &[ValueRef::Int64(VALUE)],
                                            preflight.validation_lane().unwrap(),
                                        )
                                        .unwrap();
                                        let before_wal = wal_bytes(state);
                                        let reserved = match preflight.reserve(input) {
                                            Ok(reserved) => reserved,
                                            Err(RowOwnedError::Before { error, .. })
                                            | Err(RowOwnedError::Fatal(error)) => {
                                                panic!("reserve {error:?}")
                                            }
                                        };
                                        // The RowReserve is durable before reserve returns.
                                        assert!(wal_bytes(state) > before_wal);
                                        assert_eq!(reader.catalog_test_source_used(), source);
                                        match reserved.cancel() {
                                            Ok(cancelled) => {
                                                let _ = cancelled.into_private();
                                            }
                                            Err(RowOwnedError::Before { error, .. })
                                            | Err(RowOwnedError::Fatal(error)) => {
                                                panic!("cancel {error:?}")
                                            }
                                        }
                                        assert_eq!(effects!(writer, state).0, PREVIOUS_LSN + 2);
                                    }
                                    let closed = hook::observe(core);
                                    assert!(!closed.read_gate_closed);
                                    assert!(!closed.fenced);
                                    assert_eq!(reader.catalog_test_source_used(), held_used);
                                }
                                Ok(RowPreflightStep::FlushRequired { .. }) => {
                                    panic!("every source is resident")
                                }
                            }
                        });
                    });
                    assert_eq!(reader.catalog_test_source_used(), used);
                }
                close_row!(writer, owner, row, view, private, flush);
                assert!(!fence.is_fenced());
            }
        );
    });
}
