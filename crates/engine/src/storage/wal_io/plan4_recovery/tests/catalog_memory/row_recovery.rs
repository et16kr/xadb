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
    disk::{
        IsolationLevel, RecoveryAction, RecoveryOwnedError, RollbackScope, RollbackStep,
        RowOperation, RowOwnedError, RowPreflightStep, TableStep, TransactionFinishError,
    },
    retention_page::row::{self as job3, JobScope, JobState},
    row::insert::{
        driver::recovered::handle_recovered_clr, produce_clr, produce_insert, validate_insert,
    },
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

/// One applied row of `value`; a FlushRequired preflight flushes and retries.
macro_rules! put {
    ($writer:ident, $owner:ident, $view:ident, $table:ident, $private:ident, $flush:ident, $flusher:ident, $value:expr) => {{
        loop {
            match $writer
                .preflight_row($owner, &$view, &$table, $private, &mut $flush)
                .unwrap()
            {
                RowPreflightStep::Ready(mut preflight) => {
                    let input = validate_insert(
                        &[ValueRef::Int64($value)],
                        preflight.validation_lane().unwrap(),
                    )
                    .unwrap();
                    let mut reserved = accepted!(preflight.reserve(input));
                    let proposal = produce_insert(reserved.producer_lane().unwrap()).unwrap();
                    let prepared = accepted!(reserved.prepare(proposal));
                    break accepted!(prepared.apply());
                }
                RowPreflightStep::FlushRequired { target } => {
                    flush_target!($flush, $flusher, target)
                }
            }
        }
    }};
}

/// A live S4 rollback drained through A's CLR and finished.
macro_rules! rolled_back {
    ($rollback:ident) => {{
        let mut clrs = 0;
        loop {
            let target = match $rollback.next().unwrap() {
                RollbackStep::Candidate(mut task) => {
                    let proposal = produce_clr(task.producer_lane().unwrap()).unwrap();
                    let prepared = accepted!(task.prepare(proposal));
                    assert_eq!(accepted!(prepared.apply()).operation(), RowOperation::Clr);
                    clrs += 1;
                    None
                }
                RollbackStep::FlushRequired { target } => Some(target),
                RollbackStep::Complete => break,
            };
            if let Some(target) = target {
                $rollback.flush(&target).unwrap();
            }
        }
        accepted!($rollback.finish());
        clrs
    }};
}

/// The frozen bound of a case with recovered READY restarts: the debug
/// ELF's closed step costs about 1.5 s (each validates the whole history),
/// and the longest case (S6R-M06-S, twenty traced steps) took 133 s in its
/// first run.
const READY_BOUND: Duration = Duration::from_secs(240);

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
    /// `rows` applied rows, the full RollbackStart, no CLR or AbortEnd: with
    /// no row the Aborting transaction's next is None (MCB05-N04).
    AbortingFull,
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
                Ending::AbortingFull => {
                    assert!(writer.finish_statement(owner, view).is_ok());
                    let rollback = writer
                        .start_row_rollback(
                            owner,
                            &tx,
                            None,
                            RollbackScope::Transaction,
                            private,
                            &mut flush,
                            flusher,
                        )
                        .unwrap();
                    // The crash point: RollbackStart is durable; the
                    // unfinished rollback fences the session when dropped.
                    let images = state.lock().unwrap().images();
                    drop(rollback);
                    drop(tx);
                    images
                }
            }
        }
    )
}

/// MCB05-N05: a fresh installation's original SystemPage (Undo) reservation
/// only, before its INIT or cancel: no Row or Catalog actor, no root.
fn system_reserved_image() -> Images {
    let oracle = Oracle::new();
    with_fresh!(
        oracle,
        frames = 16,
        |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
            let _ = (status, undo, private, reader_private, core, fence);
            let mut cursor = Plan4ResidencyCursor::new();
            for _ in 0..32 {
                match live.prepare_undo_slot(owner, &mut cursor).unwrap() {
                    None => break,
                    Some(RetentionResidencyStep::FlushRequired { target, .. }) => {
                        drive_flush(live, flusher, Some(&target))
                    }
                    Some(_) => panic!("unexpected System slot preparation"),
                }
            }
            let reserved = live.reserve_undo(owner).unwrap();
            let images = state.lock().unwrap().images();
            drop(reserved);
            images
        }
    )
}

/// MCB05-N06: the original Catalog CREATE reservation of table T, before
/// its Forward or cancel.
fn catalog_reserved_image() -> Images {
    let oracle = Oracle::new();
    with_fresh!(
        oracle,
        frames = 16,
        |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
            let _ = (reader_private, core, fence);
            prepare_create(live, owner, flusher, status, undo);
            let (mut writer, _, mut flush) = live.split();
            let tx = writer
                .begin_catalog_transaction(
                    owner,
                    IsolationLevel::Serializable,
                    status,
                    private,
                    &mut flush,
                )
                .unwrap();
            let view = writer.begin_statement(owner, &tx).unwrap();
            let definition = table_spec("T");
            let reserved = writer
                .reserve_catalog_create(owner, &view, undo, &definition, private, &mut flush)
                .ok()
                .unwrap();
            let images = state.lock().unwrap().images();
            let _ = reserved.cancel();
            drop(view);
            drop(tx);
            images
        }
    )
}

/// The no-loser inputs of MCB05-N01..06 (consumers `#acceptance`), each the
/// durable files of an actual writer run.
fn no_loser_input(id: &str) -> Images {
    match id {
        "MCB05-N01" => image(1, Ending::Active),
        "MCB05-N02" => image(1, Ending::Reserved),
        "MCB05-N03" => image(0, Ending::Active),
        "MCB05-N04" => image(0, Ending::AbortingFull),
        "MCB05-N05" => system_reserved_image(),
        "MCB05-N06" => catalog_reserved_image(),
        _ => panic!("no-loser input {id}"),
    }
}

/// The restart refusal of `policy` (None: the legacy entry) over `images`:
/// Analysis/Unsupported, CallerMustResolve, database Unchanged, no
/// mutation and every file kept.
fn refused(images: &Images, policy: Option<RowRecoveryPolicy>) {
    let directory = clone_directory(images);
    let state = directory.state();
    refused_in(&directory, policy);
    assert_closed(&state);
    assert_eq!(
        &state.lock().unwrap().images(),
        images,
        "every file unchanged"
    );
}

/// `refused` over any directory port; the caller checks its files.
fn refused_in<D: DirectoryPort>(directory: &D, policy: Option<RowRecoveryPolicy>) {
    let mut maintenance = boxed::<12_582_912>();
    let mut dwb = boxed::<4_194_304>();
    let mut slot = boxed::<1_048_576>();
    let owner = match Plan4Recovery::acquire(
        directory,
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
    println!("S6R_REFUSED policy={policy:?} {:?}", failed.error());
}

/// How a restart drives the closed facade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Drive {
    /// The original `step`: no Row CLR handler.
    Step,
    /// `next_action`, each RowClr through A's `handle_recovered_clr`.
    Action,
    /// As Action; the first task's first proposal has a wrong after image
    /// (S6R-M05), whose Before returns the same task for A's retry.
    WrongFirst,
    /// As Action; after this many applied CLRs the process stops: the files
    /// are taken and the facade is dropped before READY (S6R-M07).
    CrashAfter(usize),
    /// As Action; the process stops once the Row cells changed this many
    /// times (MBC40-N02: after the recovered Cancel).
    CrashAfterCells(usize),
}

/// One restart's result: the Row cell trace after each progressed action,
/// A's applied CLRs, the facade's observation at bind, the files at the
/// READY handoff (or at the crash), and B's reads of RowId 1..=n (None:
/// Missing).
struct Restart {
    trace: Vec<Cells>,
    clrs: usize,
    observation: Plan4RecoveryObservation,
    images: Images,
    reads: Vec<Option<i64>>,
}

/// One restart of `policy` (None: the legacy entry) over `images`, driven
/// as `drive` to READY, then B reads RowId 1..=`reads` under a new Row
/// snapshot (a crash drive reads nothing).
fn restart(
    images: &Images,
    policy: Option<RowRecoveryPolicy>,
    drive: Drive,
    reads: u64,
) -> Restart {
    let directory = clone_directory(images);
    let state = directory.state();
    let restarted = restart_in(
        &directory,
        &|| state.lock().unwrap().images(),
        policy,
        drive,
        reads,
    );
    assert_closed(&state);
    restarted
}

/// `restart` over any directory port; `files` takes its current files.
fn restart_in<D: DirectoryPort>(
    directory: &D,
    files: &dyn Fn() -> Images,
    policy: Option<RowRecoveryPolicy>,
    drive: Drive,
    reads: u64,
) -> Restart {
    use crate::storage::disk::{Plan4ResidencyCursor, ReadStep};
    use crate::storage::row::read::driver::{
        PointRead,
        live::{LiveCall as ReadCall, read_point_live},
    };
    use foundation::{identity::RowId, runtime::Cancellation};
    let started = std::time::Instant::now();
    let mut trace = Vec::new();
    let mut clrs = 0;
    let mut values = Vec::new();
    let observation;
    let taken;
    {
        let mut maintenance = boxed::<12_582_912>();
        let mut dwb = boxed::<4_194_304>();
        let mut slot = boxed::<1_048_576>();
        let owner = Plan4Recovery::acquire(
            directory,
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
        observation = *closed.observation();
        trace.push(row_cells(closed.inner.catalog_test_jobs()));
        let mut ready = false;
        let mut wrong = drive == Drive::WrongFirst;
        for _ in 0..512 {
            if drive == Drive::Step {
                ready = closed.step(&mut owner, &mut flusher).unwrap().ready;
            } else {
                match closed.next_action(&mut owner, &mut flusher).unwrap() {
                    RecoveryAction::Ready => ready = true,
                    RecoveryAction::Progress(progress) => assert!(!progress.ready),
                    RecoveryAction::RowClr(mut task) => {
                        if wrong {
                            wrong = false;
                            let proposal = produce_clr(task.producer_lane().unwrap()).unwrap();
                            {
                                let (_, _, _, _, _, after, _, _) =
                                    task.producer_lane().unwrap().parts();
                                after[0][100] ^= 1;
                            }
                            let before = files();
                            task = match task.prepare(proposal) {
                                Err(RecoveryOwnedError::Before { error, owned }) => {
                                    assert_eq!(error.effect, Effect::Unchanged);
                                    assert!(matches!(
                                        error.cause,
                                        Plan4RecoveryCause::Retention(
                                            crate::storage::disk::RetentionCause::RowSemantic(_)
                                        )
                                    ));
                                    owned
                                }
                                Err(RecoveryOwnedError::Fatal(error)) => panic!("{error:?}"),
                                Ok(_) => panic!("wrong after image prepared"),
                            };
                            assert!(files() == before, "a refused prepare wrote");
                            assert!(!runtime.session_fence().is_fenced());
                        }
                        let actor = task.actor();
                        match handle_recovered_clr(task) {
                            Ok(progress) => {
                                assert!(progress.progressed && !progress.ready);
                                assert_eq!(progress.phase, Plan4RecoveryPhase::Losers);
                            }
                            Err(_) => panic!("A's recovered CLR for {actor:?}"),
                        }
                        clrs += 1;
                    }
                }
            }
            let cells = row_cells(closed.inner.catalog_test_jobs());
            if trace.last() != Some(&cells) {
                trace.push(cells);
            }
            if ready
                || drive == Drive::CrashAfter(clrs)
                || drive == Drive::CrashAfterCells(trace.len() - 1)
            {
                break;
            }
        }
        if let Drive::CrashAfter(_) | Drive::CrashAfterCells(_) = drive {
            assert!(
                !ready
                    && (drive == Drive::CrashAfter(clrs)
                        || drive == Drive::CrashAfterCells(trace.len() - 1)),
                "crash point {drive:?}: {clrs} CLRs, {} traces",
                trace.len()
            );
            taken = files();
            drop(closed);
            println!("S6R_CRASH_TIME after={:?}", started.elapsed());
            return Restart {
                trace,
                clrs,
                observation,
                images: taken,
                reads: values,
            };
        }
        assert!(ready, "bounded Row recovery exhausted");
        let (mut live, private) = match closed.into_live_with_private() {
            Ok(value) => value,
            Err(_) => panic!("original READY handoff"),
        };
        assert!(row_cells(live.catalog_test_jobs()).is_empty());
        taken = files();
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
            for row in 1..=reads {
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
                values.push(match read {
                    PointRead::Found(copied) => {
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
                        match decoded.value(0).unwrap() {
                            ValueRef::Int64(value) => Some(value),
                            other => panic!("row {row}: {other:?}"),
                        }
                    }
                    PointRead::Missing => None,
                });
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
            "S6R_READY_TIME policy={policy:?} drive={drive:?} ready={readied:?} read={:?}",
            started.elapsed()
        );
    }
    Restart {
        trace,
        clrs,
        observation,
        images: taken,
        reads: values,
    }
}

/// The original-step restart to READY of S6R-M01/M02: B reads `rows`
/// committed rows of VALUE, or finds RowId 1 Missing when `rows` is 0.
/// Returns the trace and the files at the READY handoff.
fn recover_ready(
    images: &Images,
    policy: Option<RowRecoveryPolicy>,
    rows: u64,
) -> (Vec<Cells>, Images) {
    let restarted = restart(images, policy, Drive::Step, rows.max(1));
    let expected = if rows == 0 {
        vec![None]
    } else {
        vec![Some(VALUE); rows as usize]
    };
    assert_eq!(restarted.reads, expected);
    assert_eq!(restarted.clrs, 0);
    (restarted.trace, restarted.images)
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

/// The Row cell states of a trace, cursor and ordinal dropped.
fn states(trace: &[Cells]) -> Vec<Vec<JobState>> {
    trace
        .iter()
        .map(|cells| cells.iter().map(|(_, state, _)| *state).collect())
        .collect()
}

/// The aborted cleanup of `roots` compensated roots (q0): each root's three
/// ReleaseRole and RootFree, then the TerminalFinish.
fn compensated_tail(trace: &[Cells], roots: usize) {
    let tail = &trace[trace.len() - (4 * roots + 2)..];
    let first = &tail[0];
    assert_eq!(first.len(), roots + 1, "{trace:#?}");
    assert_eq!(
        first
            .iter()
            .filter(
                |(_, state, cursor)| *state == JobState::RootReleasingCompensated && *cursor == 0
            )
            .count(),
        roots,
        "{trace:#?}"
    );
    assert!(
        first
            .iter()
            .any(|(_, state, _)| *state == JobState::TerminalReady)
    );
    assert!(tail.last().unwrap().is_empty());
}

/// S6R-M04 (C+A loser): a crash of an Active three-row chain. WithRowClr
/// runs the full RollbackStart, A's handler CLR for each head, AbortEnd and
/// the compensated cleanup to READY; B finds the table empty.
#[test]
fn s6r_m04_active_chain_recovers_through_row_clr() {
    case_within("S6R-M04", READY_BOUND, || {
        let images = image(3, Ending::Active);
        let restarted = restart(
            &images,
            Some(RowRecoveryPolicy::WithRowClr),
            Drive::Action,
            3,
        );
        println!("S6R_M04 trace={:?}", restarted.trace);
        assert_eq!(restarted.clrs, 3);
        assert_eq!(restarted.reads, vec![None; 3]);
        let traced = states(&restarted.trace);
        // Three RootRetained roots and the leased terminal; RollbackStart
        // changes the Status only.
        assert_eq!(
            traced[0]
                .iter()
                .filter(|s| **s == JobState::RootRetained)
                .count(),
            3
        );
        assert!(traced[0].contains(&JobState::TerminalLeased));
        // Three CLRs, the AbortEnd, then 3 × 4 releases and TerminalFinish.
        assert_eq!(restarted.trace.len(), 1 + 3 + 1 + 12 + 1, "{traced:#?}");
        compensated_tail(&restarted.trace, 3);
        // The READY files restart READY through the legacy entry.
        let again = restart(&restarted.images, None, Drive::Step, 3);
        assert_eq!((again.trace.len(), again.reads), (1, vec![None; 3]));
    });
}

/// S6R-M05 (MBC40-V06): the first recovered task's proposal carries a wrong
/// after image. C's prepare refuses it Unchanged with no file change and no
/// fence, returns the same task, and A's retry on it succeeds.
#[test]
fn s6r_m05_wrong_proposal_returns_the_same_task() {
    case_within("S6R-M05", READY_BOUND, || {
        let images = image(1, Ending::Active);
        let restarted = restart(
            &images,
            Some(RowRecoveryPolicy::WithRowClr),
            Drive::WrongFirst,
            1,
        );
        println!("S6R_M05 trace={:?}", restarted.trace);
        assert_eq!(restarted.clrs, 1);
        assert_eq!(restarted.reads, vec![None]);
        compensated_tail(&restarted.trace, 1);
    });
}

/// An actual S4 run: `S` statement rollback of a two-row suffix between
/// committed rows 7 and 10 and the Row Commit (S4-R01), or `A` a full abort
/// of three rows from two statements and finish_aborted (S4-R10).
fn rolled_back_image(commit: bool) -> Images {
    let oracle = Oracle::new();
    with_fresh!(
        oracle,
        frames = 16,
        |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
            let _ = core;
            let (row_status, _) = create_table(live, owner, flusher, status, undo, private, true);
            for _ in 0..if commit { 3 } else { 2 } {
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
            let first = writer.begin_statement(owner, &tx).unwrap();
            let table = issue_table!(reader, owner, first, reader_private, flush, flusher);
            put!(writer, owner, first, table, private, flush, flusher, 7);
            if !commit {
                put!(writer, owner, first, table, private, flush, flusher, 8);
            }
            assert!(writer.finish_statement(owner, first).is_ok());
            let second = writer.begin_statement(owner, &tx).unwrap();
            put!(writer, owner, second, table, private, flush, flusher, 8);
            if commit {
                put!(writer, owner, second, table, private, flush, flusher, 9);
                {
                    let mut rollback = writer
                        .start_row_rollback(
                            owner,
                            &tx,
                            Some(&second),
                            RollbackScope::Statement,
                            private,
                            &mut flush,
                            flusher,
                        )
                        .unwrap();
                    assert_eq!(rolled_back!(rollback), 2);
                }
                assert!(writer.finish_statement(owner, second).is_ok());
                let third = writer.begin_statement(owner, &tx).unwrap();
                put!(writer, owner, third, table, private, flush, flusher, 10);
                assert!(writer.finish_statement(owner, third).is_ok());
                match writer.commit_row(owner, tx, private) {
                    Ok(committed) => assert_eq!(committed.q(), 2),
                    Err(_) => panic!("commit after suffix rollback"),
                }
            } else {
                assert!(writer.finish_statement(owner, second).is_ok());
                {
                    let mut rollback = writer
                        .start_row_rollback(
                            owner,
                            &tx,
                            None,
                            RollbackScope::Transaction,
                            private,
                            &mut flush,
                            flusher,
                        )
                        .unwrap();
                    assert_eq!(rolled_back!(rollback), 3);
                }
                assert!(writer.finish_aborted(owner, tx).is_ok());
            }
            assert!(!fence.is_fenced());
            state.lock().unwrap().images()
        }
    )
}

/// S6R-M06: S4's statement rollback then Commit — the committed roots'
/// Prune and the compensated q0 roots' release — and S4's full abort, each
/// cleaned to READY with no CLR.
#[test]
fn s6r_m06_statement_rollback_then_commit() {
    case_within("S6R-M06-S", READY_BOUND, || {
        let images = rolled_back_image(true);
        let restarted = restart(
            &images,
            Some(RowRecoveryPolicy::WithRowClr),
            Drive::Action,
            4,
        );
        println!(
            "S6R_M06S trace={:?} reads={:?}",
            restarted.trace, restarted.reads
        );
        assert_eq!(restarted.clrs, 0);
        let first = &restarted.trace[0];
        let count = |state| first.iter().filter(|(_, s, _)| *s == state).count();
        assert_eq!(
            (
                count(JobState::RootRetained),
                count(JobState::RootReleasingCompensated),
                count(JobState::TerminalReady)
            ),
            (2, 2, 1)
        );
        // Two Prunes, 4 × 4 releases and the TerminalFinish.
        assert_eq!(restarted.trace.len(), 1 + 2 + 16 + 1);
        assert!(restarted.trace.last().unwrap().is_empty());
        let found: Vec<i64> = restarted.reads.iter().flatten().copied().collect();
        assert_eq!(found, vec![7, 10], "{:?}", restarted.reads);
    });
}
#[test]
fn s6r_m06_full_abort() {
    case_within("S6R-M06-A", READY_BOUND, || {
        let images = rolled_back_image(false);
        let restarted = restart(
            &images,
            Some(RowRecoveryPolicy::WithRowClr),
            Drive::Action,
            3,
        );
        println!("S6R_M06A trace={:?}", restarted.trace);
        assert_eq!(restarted.clrs, 0);
        assert_eq!(restarted.trace.len(), 1 + 12 + 1);
        compensated_tail(&restarted.trace, 3);
        assert_eq!(restarted.reads, vec![None; 3]);
    });
}

/// S6R-M07: a recovered process stops after one CLR of a three-row Active
/// chain. A new process over the files at that point selects the rest of
/// the same chain from the original binding: two CLRs, not three, with the
/// first CLR's root already compensated, and its analysis ends after the
/// first process's RollbackStart and CLR frames.
#[test]
fn s6r_m07_partial_recovery_then_crash() {
    case_within("S6R-M07", READY_BOUND, || {
        let images = image(3, Ending::Active);
        let crashed = restart(
            &images,
            Some(RowRecoveryPolicy::WithRowClr),
            Drive::CrashAfter(1),
            0,
        );
        println!("S6R_M07 crash trace={:?}", crashed.trace);
        assert_eq!(crashed.clrs, 1);
        let restarted = restart(
            &crashed.images,
            Some(RowRecoveryPolicy::WithRowClr),
            Drive::Action,
            3,
        );
        println!(
            "S6R_M07 trace={:?} crashed={:?} restarted={:?}",
            restarted.trace, crashed.observation, restarted.observation
        );
        assert_eq!(restarted.clrs, 2);
        assert_eq!(restarted.reads, vec![None; 3]);
        // The new analysis consumed the first process's RollbackStart and
        // CLR frames and its appends continue after them: no LSN is reused.
        assert_eq!(
            restarted.observation.source_lsn,
            crashed.observation.source_lsn + 2
        );
        compensated_tail(&restarted.trace, 3);
        // The remaining two roots are compensated, the first already is.
        let first = &restarted.trace[0];
        let count = |state| first.iter().filter(|(_, s, _)| *s == state).count();
        assert_eq!(
            (
                count(JobState::RootRetained),
                count(JobState::RootReleasingCompensated)
            ),
            (2, 1),
            "{first:?}"
        );
    });
}

/// S6R-M08 (memory form of MCB05-N01..06): RejectInitialLosers refuses each
/// no-loser input after the canonical analysis, every file unchanged.
#[test]
fn s6r_m08_reject_initial_losers_inputs() {
    case_within("S6R-M08", READY_BOUND, || {
        for id in [
            "MCB05-N01",
            "MCB05-N02",
            "MCB05-N03",
            "MCB05-N04",
            "MCB05-N05",
            "MCB05-N06",
        ] {
            let started = std::time::Instant::now();
            refused(
                &no_loser_input(id),
                Some(RowRecoveryPolicy::RejectInitialLosers),
            );
            println!("S6R_M08 {id} refused in {:?}", started.elapsed());
        }
    });
}

#[cfg(target_os = "linux")]
#[path = "row_recovery/native.rs"]
mod native;
