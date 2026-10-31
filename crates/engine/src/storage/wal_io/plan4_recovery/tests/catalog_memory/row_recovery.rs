//! S6 C4 Row recovery READY (XADB-MODULE-ROW-RECOVERY-001, boundaries
//! `#row-recovery-s6`): restarts over actual S3/S4 images through
//! `recover_with_rows`, the closed facade's recovered Row cleanup and the
//! original flush/VERIFY/STABILIZE handoff. Each image is the durable file
//! set of an actual writer run; nothing here builds a recovery input. The
//! Row cell trace after every progressed step is the selection order the
//! boundaries fix, and READY is followed by B's read of the table under a
//! new Row snapshot.
use super::super::super::{Plan4RecoveryAcquireFailure, RowRecoveryPolicy};
use super::super::faults::unchanged_error;
use super::super::support::namespace;
use super::row_forward::VALUE;
use super::row_preflight::{create_table, system_init};
use super::*;
use crate::storage::{
    disk::{IsolationLevel, RowOwnedError, RowPreflightStep, TableStep, TransactionFinishError},
    retention_page::row::{self as job3, JobScope, JobState},
    row::insert::{produce_insert, validate_insert},
    row_record::ValueRef,
};
use std::time::Duration;

macro_rules! accepted {
    ($call:expr) => {
        match $call {
            Ok(value) => value,
            Err(RowOwnedError::Before { error, .. }) | Err(RowOwnedError::Fatal(error)) => {
                panic!("Row operation {error:?}")
            }
        }
    };
}

macro_rules! ready {
    ($writer:ident, $owner:ident, $view:ident, $table:ident, $private:ident, $flush:ident) => {
        match $writer
            .preflight_row($owner, &$view, &$table, $private, &mut $flush)
            .unwrap()
        {
            RowPreflightStep::Ready(preflight) => preflight,
            RowPreflightStep::FlushRequired { .. } => panic!("settled sources"),
        }
    };
}

/// One applied row of VALUE through A's validate/produce and C's
/// reserve/prepare/apply, after its sources are settled.
macro_rules! inserted {
    ($writer:ident, $owner:ident, $view:ident, $table:ident, $private:ident, $flush:ident, $flusher:ident) => {{
        let _ = settle!($writer, $owner, $view, $table, $private, $flush, $flusher);
        let mut preflight = ready!($writer, $owner, $view, $table, $private, $flush);
        let plan = validate_insert(
            &[ValueRef::Int64(VALUE)],
            preflight.validation_lane().unwrap(),
        )
        .unwrap();
        let mut reserved = accepted!(preflight.reserve(plan));
        let proposal = produce_insert(reserved.producer_lane().unwrap()).unwrap();
        let prepared = accepted!(reserved.prepare(proposal));
        accepted!(prepared.apply());
    }};
}

/// The frozen bound of a case with recovered READY restarts: the debug
/// ELF's closed step costs about 1.5 s (each validates the whole history).
const READY_BOUND: Duration = Duration::from_secs(150);

/// The Row cells of the bank: (ordinal, state, release cursor).
type Cells = Vec<(usize, JobState, u8)>;

fn row_cells(jobs: &[[u8; 1024]; 64]) -> Cells {
    jobs.iter()
        .enumerate()
        .filter_map(|(ordinal, bytes)| {
            let job = job3::decode_job(bytes, namespace()).unwrap();
            (job.scope() == Some(JobScope::Row) && job.state() != JobState::Free)
                .then(|| (ordinal, job.state(), job.release_cursor()))
        })
        .collect()
}

/// How the actual writer run ends before the crash image is taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ending {
    /// `rows` applied rows and the Row Commit (S3).
    Committed,
    /// `rows - 1` applied rows, then one RowReserve only (S2-b2).
    Reserved,
    /// `rows` applied rows of an Active statement, no Commit.
    Active,
}

/// The durable file set after an actual Row run on table T: `rows` Free
/// Undo pages are INIT first, so each row has its own U.
fn image(rows: usize, ending: Ending) -> Images {
    let oracle = Oracle::new();
    with_fresh!(
        oracle,
        frames = 16,
        |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
            let _ = core;
            let (row_status, _) = create_table(live, owner, flusher, status, undo, private, true);
            for _ in 1..rows {
                system_init(live, owner, flusher, true);
            }
            let (mut writer, reader, mut flush) = live.split();
            let tx = writer
                .begin_row_transaction(
                    owner,
                    IsolationLevel::Serializable,
                    &row_status,
                    private,
                    &mut flush,
                )
                .unwrap();
            let view = writer.begin_statement(owner, &tx).unwrap();
            let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
            let applied = if ending == Ending::Reserved {
                rows - 1
            } else {
                rows
            };
            for _ in 0..applied {
                inserted!(writer, owner, view, table, private, flush, flusher);
            }
            match ending {
                Ending::Committed => {
                    assert!(writer.finish_statement(owner, view).is_ok());
                    match writer.commit_row(owner, tx, private) {
                        Ok(committed) => assert_eq!(committed.q(), 2),
                        Err(TransactionFinishError::Before { error, .. })
                        | Err(TransactionFinishError::RollbackRequired { error, .. })
                        | Err(TransactionFinishError::Rejected(error))
                        | Err(TransactionFinishError::Fatal(error)) => panic!("Commit {error:?}"),
                    }
                    let cells = row_cells(writer.catalog_test_jobs());
                    assert_eq!(cells.len(), rows + 1, "{cells:?}");
                    assert!(!fence.is_fenced());
                    state.lock().unwrap().images()
                }
                Ending::Reserved => {
                    let _ = settle!(writer, owner, view, table, private, flush, flusher);
                    let mut preflight = ready!(writer, owner, view, table, private, flush);
                    let plan = validate_insert(
                        &[ValueRef::Int64(VALUE)],
                        preflight.validation_lane().unwrap(),
                    )
                    .unwrap();
                    let reserved = accepted!(preflight.reserve(plan));
                    // The crash point: RowReserve is durable, its Forward is
                    // not staged.
                    let images = state.lock().unwrap().images();
                    accepted!(reserved.cancel());
                    drop(view);
                    drop(tx);
                    images
                }
                Ending::Active => {
                    let images = state.lock().unwrap().images();
                    drop(view);
                    drop(tx);
                    images
                }
            }
        }
    )
}

/// The restart refusal of `policy` (None: the legacy entry) over `images`:
/// Analysis/Unsupported, CallerMustResolve, database Unchanged, no
/// mutation and every file kept.
fn refused(images: &Images, policy: Option<RowRecoveryPolicy>) {
    let directory = clone_directory(images);
    let state = directory.state();
    {
        let mut maintenance = boxed::<12_582_912>();
        let mut dwb = boxed::<4_194_304>();
        let mut slot = boxed::<1_048_576>();
        let owner = match Plan4Recovery::acquire(
            &directory,
            Some(namespace()),
            &mut maintenance,
            &mut dwb,
            &mut slot,
        ) {
            Ok(owner) => owner,
            Err(Plan4RecoveryAcquireFailure::Acquired(_))
            | Err(Plan4RecoveryAcquireFailure::Before { .. }) => panic!("acquire"),
        };
        let result = match policy {
            None => owner.recover(),
            Some(policy) => owner.recover_with_rows(policy),
        };
        let failed = match result {
            Err(failed) => failed,
            Ok(_) => panic!("{policy:?} admitted a refused Row image"),
        };
        assert_eq!(failed.error().effect, Effect::CallerMustResolve);
        assert!(failed.error().lease_acquired);
        unchanged_error(
            failed.error(),
            failed.observation(),
            Plan4RecoveryPhase::Analysis,
        );
        assert!(matches!(
            failed.error().cause,
            Plan4RecoveryCause::Unsupported
        ));
        drop(failed);
        assert_closed(&state);
    }
    assert_eq!(
        &state.lock().unwrap().images(),
        images,
        "every file unchanged"
    );
}

/// One restart of `policy` over `images` driven to READY through the
/// closed facade's `step`, then B reads `rows` committed rows of VALUE (or
/// finds the table empty) under a new Row snapshot. Returns the Row cell
/// trace after each progressed step and the files at the READY handoff,
/// before B's Row transaction.
fn recover_ready(
    images: &Images,
    policy: Option<RowRecoveryPolicy>,
    rows: u64,
) -> (Vec<Cells>, Images) {
    use crate::storage::disk::{Plan4ResidencyCursor, ReadStep};
    use crate::storage::row::read::driver::{
        PointRead,
        live::{LiveCall as ReadCall, read_point_live},
    };
    use foundation::{identity::RowId, runtime::Cancellation};
    let started = std::time::Instant::now();
    let directory = clone_directory(images);
    let state = directory.state();
    let mut trace = Vec::new();
    let ready_images;
    {
        let mut maintenance = boxed::<12_582_912>();
        let mut dwb = boxed::<4_194_304>();
        let mut slot = boxed::<1_048_576>();
        let owner = Plan4Recovery::acquire(
            &directory,
            Some(namespace()),
            &mut maintenance,
            &mut dwb,
            &mut slot,
        )
        .ok()
        .unwrap();
        let runtime = match policy {
            None => owner.recover(),
            Some(policy) => owner.recover_with_rows(policy),
        }
        .ok()
        .unwrap();
        let core = PageConcurrency::new(runtime.session_fence()).unwrap();
        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
        let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let mut backing = vec![[0xa7; PAGE_BYTES]; 16];
        let mut frames: Vec<_> = backing.iter_mut().map(DirtyFrame::new).collect();
        let mut operation = boxed::<114_688>();
        let mut closed = runtime
            .bind(&owner, &core, &mut frames, &mut operation)
            .unwrap();
        trace.push(row_cells(closed.inner.catalog_test_jobs()));
        let mut ready = false;
        for _ in 0..256 {
            let progress = closed.step(&mut owner, &mut flusher).unwrap();
            let cells = row_cells(closed.inner.catalog_test_jobs());
            if trace.last() != Some(&cells) {
                trace.push(cells);
            }
            if progress.ready {
                ready = true;
                break;
            }
        }
        assert!(ready, "bounded Row recovery exhausted");
        let (mut live, private) = match closed.into_live_with_private() {
            Ok(value) => value,
            Err(_) => panic!("original READY handoff"),
        };
        assert!(row_cells(live.catalog_test_jobs()).is_empty());
        ready_images = Some(state.lock().unwrap().images());
        let row_status = live.recovery_test_expected_target(11).unwrap();
        let readied = started.elapsed();
        // B's Row Begin takes its Root, first Jobs page and Status resident
        // through the original empty-transaction preparation.
        let first_jobs = crate::storage::retention_page::JobSlotRef::new(
            page_ref(2, 1),
            0,
            foundation::identity::Generation::new(1).unwrap(),
        )
        .unwrap();
        let mut cursor = Plan4ResidencyCursor::new();
        for _ in 0..16 {
            match live
                .prepare_empty_targets(&owner, &row_status, first_jobs, &mut cursor)
                .unwrap()
            {
                None => break,
                Some(RetentionResidencyStep::FlushRequired { target, .. }) => {
                    drive_flush(&mut live, &mut flusher, Some(&target))
                }
                Some(_) => panic!("unexpected Begin preparation"),
            }
        }
        // B's table issue reads the Catalog anchor and leaf.
        prepare_catalog(&mut live, &owner, &mut flusher);
        let mut reader_private = boxed::<PRIVATE_BYTES>();
        let reader_private = &mut *reader_private;
        let owner = &mut owner;
        let flusher = &mut flusher;
        {
            let (mut writer, reader, mut flush) = live.split();
            let tx = writer
                .begin_row_transaction(
                    owner,
                    IsolationLevel::Serializable,
                    &row_status,
                    private,
                    &mut flush,
                )
                .unwrap();
            let view = writer.begin_statement(owner, &tx).unwrap();
            let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
            let mut residency = Plan4ResidencyCursor::new();
            let cancel = Cancellation::default();
            let mut output = [0xA5; 64];
            for row in 1..=rows.max(1) {
                let read = loop {
                    match read_point_live(
                        ReadCall {
                            port: &reader,
                            owner,
                            private: reader_private,
                            residency: &mut residency,
                        },
                        &view,
                        &table,
                        RowId::new(row).unwrap(),
                        &mut output,
                        &cancel,
                    )
                    .unwrap()
                    {
                        ReadStep::Ready(read) => break read,
                        ReadStep::FlushRequired { target } => {
                            flush_target!(flush, flusher, target)
                        }
                    }
                };
                match read {
                    PointRead::Found(copied) if rows > 0 => {
                        assert_eq!(copied.version().row.get(), row);
                        let decoded = crate::storage::row_record::decode_canonical_row(
                            &output[..copied.len()],
                            &[crate::api::Column {
                                name: "i".into(),
                                data_type: crate::api::DataType::Int64,
                                nullable: false,
                            }],
                        )
                        .unwrap();
                        assert_eq!(decoded.value(0).unwrap(), ValueRef::Int64(VALUE));
                    }
                    PointRead::Missing if rows == 0 => {}
                    _ => panic!("row {row} of {rows} after READY"),
                }
            }
            assert!(writer.finish_statement(owner, view).is_ok());
            assert!(
                writer
                    .abort_empty_transaction(owner, tx, private, &mut flush)
                    .is_ok()
            );
            owner.check_idle().unwrap();
        }
        assert!(!runtime.session_fence().is_fenced());
        drop(live);
        println!(
            "S6R_READY_TIME policy={policy:?} ready={readied:?} read={:?}",
            started.elapsed()
        );
    }
    assert_closed(&state);
    (trace, ready_images.expect("READY handoff"))
}

/// The committed cleanup of one committed actor with `rows` roots
/// (consumers `#row-states`): each root's Prune (head order), three
/// ReleaseRole and RootFree, then the TerminalFinish. Flush and VERIFY
/// steps leave the cells unchanged and are not traced.
fn committed_trace(trace: &[Cells], rows: usize) {
    assert_eq!(trace.len(), 1 + 5 * rows + 1, "{trace:#?}");
    let first = &trace[0];
    assert_eq!(first.len(), rows + 1);
    assert_eq!(
        first
            .iter()
            .filter(|(_, state, _)| *state == JobState::RootRetained)
            .count(),
        rows
    );
    assert!(trace.last().unwrap().is_empty());
}

/// S6R-M01 (MBC40-V03): an actual S3 Commit of `rows` rows, then a restart
/// under `policy` to READY, B's read of the committed rows, and the legacy
/// restart of the READY files (every Row cell Free) is READY too.
fn committed_restart(id: &str, rows: usize, policy: RowRecoveryPolicy) {
    case_within(id, READY_BOUND, || {
        let started = std::time::Instant::now();
        let images = image(rows, Ending::Committed);
        let built = started.elapsed();
        let (trace, after) = recover_ready(&images, Some(policy), rows as u64);
        let recovered = started.elapsed();
        println!("S6R_M01 rows={rows} policy={policy:?} trace={trace:?}");
        committed_trace(&trace, rows);
        let (again, _) = recover_ready(&after, None, rows as u64);
        assert_eq!(again.len(), 1, "{again:?}");
        println!(
            "S6R_M01_TIME rows={rows} policy={policy:?} image={built:?} ready={recovered:?} total={:?}",
            started.elapsed()
        );
    });
}

#[test]
fn s6r_m01_one_committed_row_with_row_clr() {
    committed_restart("S6R-M01-1W", 1, RowRecoveryPolicy::WithRowClr);
}
#[test]
fn s6r_m01_one_committed_row_reject_initial_losers() {
    committed_restart("S6R-M01-1R", 1, RowRecoveryPolicy::RejectInitialLosers);
}
#[test]
fn s6r_m01_three_committed_rows_with_row_clr() {
    committed_restart("S6R-M01-3W", 3, RowRecoveryPolicy::WithRowClr);
}
#[test]
fn s6r_m01_three_committed_rows_reject_initial_losers() {
    committed_restart("S6R-M01-3R", 3, RowRecoveryPolicy::RejectInitialLosers);
}

/// S6R-M02 (MBC40-V04): a crash with only the RowReserve durable.
#[test]
fn s6r_m02_reserved_row_is_refused_without_row_clr() {
    case("S6R-M02-R", || {
        let images = image(1, Ending::Reserved);
        refused(&images, None);
        refused(&images, Some(RowRecoveryPolicy::RejectInitialLosers));
    });
}
#[test]
fn s6r_m02_reserved_row_recovers_with_row_clr() {
    case_within("S6R-M02-W", READY_BOUND, || {
        let images = image(1, Ending::Reserved);
        let (trace, _) = recover_ready(&images, Some(RowRecoveryPolicy::WithRowClr), 0);
        println!("S6R_M02 trace={trace:?}");
        let states: Vec<Vec<JobState>> = trace
            .iter()
            .map(|cells| cells.iter().map(|(_, state, _)| *state).collect())
            .collect();
        // Cancel, RollbackStart (the Status only), AbortEnd, TerminalFinish.
        assert_eq!(
            states,
            vec![
                vec![JobState::TerminalLeased, JobState::RootReserved],
                vec![JobState::TerminalLeased],
                vec![JobState::TerminalReady],
                vec![],
            ],
        );
    });
}

/// S6R-M03 (MBC40-V05, legacy block): an Active three-row chain is refused
/// by RejectInitialLosers, and the legacy entry refuses a committed Row DB
/// as it did before S6.
#[test]
fn s6r_m03_active_chain_and_legacy_committed_are_refused() {
    case("S6R-M03", || {
        let images = image(3, Ending::Active);
        refused(&images, None);
        refused(&images, Some(RowRecoveryPolicy::RejectInitialLosers));
        refused(&image(1, Ending::Committed), None);
    });
}
