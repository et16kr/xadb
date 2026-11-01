//! S10 MCA03 native worker (consumers §9.1 MCA03-N01..N09 and R01..R12, the
//! cuts of consumers §9.4; boundaries `#row-mi03-s10`), C+A only. Every case
//! is one immutable test ELF over its own namespace, one process per phase:
//!
//! - `produce` (P1) is the actual native writer (CREATE of T, then A's
//!   driver) stopped at the case's cut. A cut is either this adapter's
//!   armed I/O event (`native::arm_row_cut`: before or after one exact
//!   syscall, a first-byte write, or an injected write/sync error) or C's
//!   cfg(test) hook (`catalog_test_support::arm_abort`); either aborts the
//!   process there, with no unwinding, Drop or further namespace syscall.
//! - `crash` (R01..R12 only) is C's recovery with A's recovered CLR handler,
//!   aborted at one REBASE/epoch/UNDO barrier (R12: after READY and A's new
//!   ACK).
//! - `recover` is C's recovery (WithRowClr) to READY, the page reader over
//!   the READY files, then A's new row and its Commit ACK.
//! - `again` (MCA04-N01 only) is a further recovery (RejectInitialLosers),
//!   then A's next new row and its ACK: the issuer after a reopen.
//! - `reenter` is a further process whose recovery (RejectInitialLosers) and
//!   page read find every ACKed row.
//!
//! The receipt is written line by line as the history runs, so the issued
//! RowIds, each ACK and a refused Commit survive the abort. The expected
//! rows, CLR counts, RowId non-reuse and the winner/loser of an UNKNOWN
//! Commit are the controller's (`scripts/check_dev3_s10a_native.py`), from
//! the history and its own WAL frame reader; this worker only reports.
use super::super::super::super::super::plan4_pages::native::{self, RowCutMode, RowStep};
use super::super::super::super::RowRecoveryPolicy;
use super::super::row_forward::VALUE;
use super::super::row_native_a::{NEW, PageScan, join_restart_with};
use super::super::row_reopen::db_images as images;
use super::*;
use crate::storage::row::insert::driver::live::start_full_rollback_live;
use crate::transaction::concurrency::catalog_test_support::{self as hook, Point};
use std::{
    fmt::Display,
    fs::{File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

/// The receipt: one `key\tvalue` line per fact, written unbuffered.
struct Receipt(File);
impl Receipt {
    fn line(&mut self, key: &str, value: impl Display) {
        self.0
            .write_all(format!("{key}\t{value}\n").as_bytes())
            .unwrap();
    }
}

/// Ends the process at a worker-side cut: after the facts the receipt holds,
/// with no Drop, flush or shutdown of the session.
fn kill(receipt: &mut Receipt, at: &str) {
    receipt.line("cut", at);
    std::process::abort();
}

fn ids(rows: &[RowMutationObservation]) -> String {
    rows.iter()
        .map(|row| row.row().get().to_string())
        .collect::<Vec<_>>()
        .join(",")
}

const WAL: &str = "d3-wal-";
const HOME: &str = "d3-data-";
const fn step(operation: &'static str, prefix: &'static str) -> RowStep {
    RowStep {
        operation,
        prefix,
        group_op: None,
    }
}

/// The REBASE/epoch/UNDO barrier of R01..R11 in C's recovery
/// (`plan4_recovery/epoch.rs` `rebase`, `retire_source`, `log_open`, then
/// the UNDO of the loser): the steps before it and the cut.
fn barrier(id: &str) -> (&'static str, Vec<RowStep>, RowStep, RowCutMode) {
    let current = step("publish", "d3-current-stage");
    match id {
        // The new base copy is written, not yet synced.
        "MCA03-R01" => (
            "R01_BASE_WRITTEN",
            vec![step("write", "d3-rebase-")],
            step("sync", "d3-rebase-"),
            RowCutMode::Before,
        ),
        // The base copy is synced and read back: the next mutation is the
        // CURRENT stage of the new base.
        "MCA03-R02" => (
            "R02_BASE_READBACK",
            vec![step("sync", "d3-rebase-"), step("dirsync", "namespace")],
            step("create", "d3-current-stage"),
            RowCutMode::Before,
        ),
        // CURRENT (new base, old previous) published and its directory synced.
        "MCA03-R03" => (
            "R03_CURRENT_NEW_OLD",
            vec![current, current],
            step("dirsync", "namespace"),
            RowCutMode::After,
        ),
        // CURRENT (new, new) published and synced.
        "MCA03-R04" => (
            "R04_CURRENT_NEW_NEW",
            vec![current, current, current],
            step("dirsync", "namespace"),
            RowCutMode::After,
        ),
        // The second base copy and the CURRENT naming it are synced and
        // read back: the next mutation is the old WAL's unlink.
        "MCA03-R05" => (
            "R05_SECOND_COPY_CLOSURE",
            vec![
                step("sync", "d3-rebase-"),
                step("sync", "d3-rebase-"),
                current,
            ],
            step("remove", WAL),
            RowCutMode::Before,
        ),
        // The selected old WAL is unlinked, its directory not yet synced.
        "MCA03-R06" => (
            "R06_OLD_WAL_UNLINKED",
            vec![],
            step("remove", WAL),
            RowCutMode::After,
        ),
        // The unlink's directory sync completed.
        "MCA03-R07" => (
            "R07_OLD_WAL_DIRSYNC",
            vec![step("remove", WAL)],
            step("dirsync", "namespace"),
            RowCutMode::After,
        ),
        // The new epoch's intent is in CURRENT; the epoch file is not created.
        "MCA03-R08" => (
            "R08_EPOCH_INTENT",
            vec![step("remove", WAL)],
            step("create", WAL),
            RowCutMode::Before,
        ),
        // The new epoch's header is synced; WRITER is not published.
        "MCA03-R09" => (
            "R09_EPOCH_HEADER",
            vec![step("create", WAL), step("sync", WAL)],
            step("publish", "d3-writer-stage"),
            RowCutMode::Before,
        ),
        // WRITER and CURRENT WalOpen are published, synced and read back:
        // the next mutation is the UNDO's first WAL write.
        "MCA03-R10" => (
            "R10_WAL_OPEN",
            vec![step("publish", "d3-writer-stage"), current],
            step("write", WAL),
            RowCutMode::Before,
        ),
        // UNDO complete: the loser's AbortEnd is synced; the cleanup and
        // STABILIZE/VERIFY have not run.
        "MCA03-R11" => (
            "R11_UNDO_COMPLETE",
            vec![
                step("publish", "d3-writer-stage"),
                RowStep {
                    operation: "write",
                    prefix: WAL,
                    group_op: Some(ABORT_END_OP),
                },
            ],
            step("sync", WAL),
            RowCutMode::After,
        ),
        _ => panic!("case {id}"),
    }
}

/// The page numbers the produce histories can hold (all resident in 24
/// frames).
const PAGES: u64 = 40;

/// The scope3 group op of a Row AbortEnd frame (the controller's reader
/// checks the same op at the cut).
const ABORT_END_OP: u16 = 7;

#[test]
#[ignore = "bounded S10 native controller supplies isolated paths and identity"]
fn s10a_native_worker() {
    let env = |name| std::env::var(format!("XADB_S10A_NATIVE_{name}")).unwrap();
    let (id, phase) = (env("CASE"), env("PHASE"));
    let db = PathBuf::from(env("DB"));
    let output = PathBuf::from(env("OUTPUT"));
    assert!(db.is_absolute() && output.is_absolute() && db != output);
    let source = std::env::var("XADB_CATALOG_SOURCE_SHA256").unwrap();
    let elf = std::env::var("XADB_CATALOG_ELF_SHA256").unwrap();
    assert!(valid_hash(&source) && valid_hash(&elf));
    let (directory, trace) = native::open_native(
        &db,
        &output.join("effects.tsv"),
        &output.join("entropy.tsv"),
        false,
    );
    let mut receipt = Receipt(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output.join("receipt.tsv"))
            .unwrap(),
    );
    receipt.line("case", &id);
    receipt.line("phase", &phase);
    receipt.line("pid", std::process::id());
    receipt.line("source", &source);
    receipt.line("elf", &elf);
    case_within(&format!("{id}-{phase}"), Duration::from_secs(180), || {
        match phase.as_str() {
            "produce" => produce(&id, &directory, &trace, &mut receipt),
            "crash" if id == "MCA03-R12" => {
                let joined = restart(&directory, &db, RowRecoveryPolicy::WithRowClr, true);
                report(&mut receipt, &joined);
                kill(&mut receipt, "R12_AFTER_NEW_ACK");
            }
            "crash" => {
                let (name, steps, cut, mode) = barrier(&id);
                native::arm_row_cut(&trace, name, &steps, cut, mode);
                receipt.line("armed", name);
                restart(&directory, &db, RowRecoveryPolicy::WithRowClr, true);
            }
            "recover" => {
                let joined = restart(&directory, &db, RowRecoveryPolicy::WithRowClr, true);
                report(&mut receipt, &joined);
            }
            // MCA04-N01: a further reopen, then a new row and its ACK.
            "again" => {
                let joined = restart(
                    &directory,
                    &db,
                    RowRecoveryPolicy::RejectInitialLosers,
                    true,
                );
                report(&mut receipt, &joined);
            }
            "reenter" => {
                let joined = restart(
                    &directory,
                    &db,
                    RowRecoveryPolicy::RejectInitialLosers,
                    false,
                );
                report(&mut receipt, &joined);
            }
            _ => panic!("phase {phase}"),
        }
    });
    // A cut phase that returns did not reach its cut.
    assert!(
        matches!(phase.as_str(), "recover" | "again" | "reenter"),
        "{id} {phase}: the cut was not reached"
    );
    drop(directory);
    let traced = native::native_receipt(&trace);
    assert_eq!(traced.handles, 0);
    assert!(traced.peak <= 3 && traced.fd_peak <= 6);
    receipt.line("handles", traced.handles);
    receipt.line("peak", traced.peak);
    receipt.line("fd_peak", traced.fd_peak);
    receipt.line("result", "ready");
}

fn restart<D: DirectoryPort>(
    directory: &D,
    db: &std::path::Path,
    policy: RowRecoveryPolicy,
    ack: bool,
) -> super::super::row_native_a::Joined {
    join_restart_with(
        directory,
        policy,
        ack.then_some(NEW),
        &|| images(db),
        &PageScan,
    )
}

fn report(receipt: &mut Receipt, joined: &super::super::row_native_a::Joined) {
    receipt.line("steps", joined.steps);
    receipt.line("clrs", joined.clrs);
    receipt.line(
        "rows",
        joined
            .rows
            .iter()
            .map(|(row, value)| format!("{row}:{value}"))
            .collect::<Vec<_>>()
            .join(","),
    );
    receipt.line("q_ready", joined.ready.0);
    receipt.line("row_cells", joined.row_cells);
    if let Some((row, q)) = joined.acked {
        receipt.line("ack_row", row);
        receipt.line("ack_q", q);
    }
}

/// The (A1 rows, A2 rows) of a case's history: A1 commits first (the
/// winner W), A2 is the transaction the cut stops. N09 is S9A-N03's history
/// (A2 three rows, Y one row and its Commit, A2's refused Commit and full
/// rollback).
fn history(id: &str) -> (Vec<i64>, Vec<i64>) {
    let w = vec![VALUE];
    match id {
        "MCA03-N01" | "MCA04-N01" => (vec![VALUE, VALUE + 1], vec![]),
        "MCA03-N02" => (w, vec![VALUE + 1, VALUE + 2]),
        id if id.starts_with("MCA03-N09") => (vec![VALUE + 3], vec![VALUE, VALUE + 1, VALUE + 2]),
        id if id.starts_with("MCA03-R") => (w, vec![VALUE + 1, VALUE + 2]),
        _ => (w, vec![VALUE + 1]),
    }
}

/// PID1: the history of `id`, stopped at its cut.
fn produce<D: DirectoryPort>(
    id: &str,
    directory: &D,
    trace: &Arc<Mutex<native::Trace>>,
    receipt: &mut Receipt,
) {
    let (w_values, a_values) = history(id);
    with_directory!(
        directory,
        frames = 24,
        |live, owner, flusher, status, undo, private, reader_private, core, fence| {
            let (row_status, _) = create_table(live, owner, flusher, status, undo, private, true);
            let other_status = system_init(live, owner, flusher, false);
            // One Free Undo per row (a committed or compensated root keeps its
            // container until recovery's cleanup).
            for _ in 1..w_values.len() + a_values.len() {
                system_init(live, owner, flusher, true);
            }
            // The resident targets before the split: N02 steals T's RowLeaf, and
            // N05-Durable flushes the numerically first page the Forward changed.
            // (Only registered frames: a request for a free frame is fatal.)
            let resident: Vec<Plan4Target> = (1..=PAGES)
                .filter_map(|number| resident_index(live, number))
                .map(|index| live.row_test_resident_target(index).unwrap())
                .collect();
            assert!(!fence.is_fenced());
            let (mut writer, reader, mut flush) = live.split();
            let pool = reader.catalog_test_pool();
            if id.starts_with("MCA03-N09") {
                // S9A-N03's history: A2 (Serializable) inserts three rows, Y
                // commits one row of T, A2's Commit is refused, then A2's full
                // rollback from its RollbackStart.
                let a2 = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let s1 = writer.begin_statement(owner, &a2).unwrap();
                let table = issue_table!(reader, owner, s1, reader_private, flush, flusher);
                let rows = a_insert!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    s1,
                    &table,
                    private,
                    a_values
                );
                receipt.line("a2_rows", ids(&rows));
                let y = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::ReadCommitted,
                        &other_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let yv = writer.begin_statement(owner, &y).unwrap();
                let y_rows = a_insert!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    yv,
                    &table,
                    private,
                    w_values
                );
                let y_q = a_commit!(&mut writer, owner, flush, flusher, y, private);
                receipt.line("w_rows", ids(&y_rows));
                receipt.line("ack_q", y_q);
                let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                let mut pending = match commit_live(&mut call, a2, private) {
                    Err(DriveError { failure, after }) => {
                        assert!(matches!(
                            disk(&failure).cause,
                            RetentionCause::SerializationConflict
                        ));
                        rollback_required(after)
                    }
                    Ok(_) => panic!("conflicting Commit succeeded"),
                };
                let mut rollback = match start_full_rollback_live(&mut call, &mut pending, private)
                {
                    Ok(rollback) => rollback,
                    Err(failure) => panic!("full RollbackStart {failure:?}"),
                };
                receipt.line("rollback_start", "durable");
                // The k-th CLR of the three (V01 first, V02 middle, V03 last).
                let k: usize = id[11..13].parse().unwrap();
                let (name, steps, mode) = if id.ends_with("RamOnly") {
                    (
                        "N09_CLR_RAM_ONLY",
                        vec![step("write", WAL); k - 1],
                        RowCutMode::Before,
                    )
                } else {
                    (
                        "N09_CLR_DURABLE",
                        vec![step("sync", WAL); k - 1],
                        RowCutMode::After,
                    )
                };
                let cut = if mode == RowCutMode::Before {
                    step("write", WAL)
                } else {
                    step("sync", WAL)
                };
                native::arm_row_cut(trace, name, &steps, cut, mode);
                receipt.line("armed", format!("{name}:{k}"));
                for _ in 0..3 {
                    assert!(matches!(
                        rollback_step_live(&mut rollback),
                        Ok(Stepped::Compensated)
                    ));
                }
                let _ = (fence, core, pool, resident);
                return;
            }
            // A1 = W: its rows and its Commit ACK.
            let a1 = writer
                .begin_row_transaction(
                    owner,
                    IsolationLevel::Serializable,
                    &row_status,
                    private,
                    &mut flush,
                )
                .unwrap();
            let s1 = writer.begin_statement(owner, &a1).unwrap();
            let table = issue_table!(reader, owner, s1, reader_private, flush, flusher);
            let w_rows = a_insert!(
                &mut writer,
                owner,
                flush,
                flusher,
                s1,
                &table,
                private,
                w_values
            );
            let w_q = a_commit!(&mut writer, owner, flush, flusher, a1, private);
            receipt.line("w_rows", ids(&w_rows));
            receipt.line("ack_q", w_q);
            if id == "MCA03-N01" || id == "MCA04-N01" {
                // The no-force Commit ACK received; no home page was flushed.
                kill(receipt, "N01_AFTER_ACK");
                return;
            }
            // A2, the transaction the cut stops.
            let a2 = writer
                .begin_row_transaction(
                    owner,
                    IsolationLevel::Serializable,
                    &other_status,
                    private,
                    &mut flush,
                )
                .unwrap();
            let s2 = writer.begin_statement(owner, &a2).unwrap();
            let table = issue_table!(reader, owner, s2, reader_private, flush, flusher);
            // The cuts inside A2's statement (Reserve, Forward).
            let inside: Option<(&'static str, Vec<RowStep>, RowStep, RowCutMode)> = match id {
                "MCA03-N03-RamOnly" => Some((
                    "N03_RESERVE_RAM_ONLY",
                    vec![],
                    step("write", WAL),
                    RowCutMode::Before,
                )),
                "MCA03-N03-Durable" => Some((
                    "N03_RESERVE_DURABLE",
                    vec![],
                    step("sync", WAL),
                    RowCutMode::After,
                )),
                "MCA03-N04" => Some((
                    "N04_FORWARD_FIRST_BYTE",
                    vec![step("sync", WAL)],
                    step("write", WAL),
                    RowCutMode::FirstByte,
                )),
                _ => None,
            };
            if let Some((name, steps, cut, mode)) = &inside {
                native::arm_row_cut(trace, name, steps, *cut, *mode);
                receipt.line("armed", name);
            }
            if id == "MCA03-N05-RamOnly" {
                // Hit 0 is the Reserve's install, hit 1 the Forward's.
                native::note_arm(trace, "N05_FORWARD_FIRST_RAM_PAGE");
                receipt.line("armed", "N05_FORWARD_FIRST_RAM_PAGE");
                hook::arm_abort(core, Point::ExistingInstalledFirstPage, 1);
            }
            let a_rows = a_insert!(
                &mut writer,
                owner,
                flush,
                flusher,
                s2,
                &table,
                private,
                a_values
            );
            receipt.line("a2_rows", ids(&a_rows));
            assert!(
                inside.is_none() && id != "MCA03-N05-RamOnly",
                "{id}: cut not reached"
            );
            match id {
                "MCA03-N02" | "MCA03-N05-Durable" => {
                    let target = if id == "MCA03-N02" {
                        // A2's rows are on T's RowLeaf: steal it.
                        *resident
                            .iter()
                            .find(|target| target.kind() == PageKind::RowLeaf)
                            .unwrap()
                    } else {
                        // The pages the Forward changed carry its LSN, the
                        // greatest page LSN in the pool; flush the numerically
                        // first of them.
                        let mut bytes = boxed::<PAGE_BYTES>();
                        let pages: Vec<(u64, u64)> = (1..=PAGES)
                            .filter_map(|number| {
                                let index = probe::registered_index(pool, number)?;
                                let snapshot = probe::diagnose(pool, index, &mut bytes);
                                snapshot
                                    .dirty
                                    .then_some((snapshot.frame.page_lsn.0, number))
                            })
                            .collect();
                        let forward = pages.iter().map(|(lsn, _)| *lsn).max().unwrap();
                        let first = pages
                            .iter()
                            .filter(|(lsn, _)| *lsn == forward)
                            .map(|(_, page)| *page)
                            .min()
                            .unwrap();
                        receipt.line(
                            "forward_pages",
                            pages
                                .iter()
                                .filter(|(lsn, _)| *lsn == forward)
                                .map(|(_, page)| page.to_string())
                                .collect::<Vec<_>>()
                                .join(","),
                        );
                        *resident
                            .iter()
                            .find(|target| target.identity().page.page_no == first)
                            .unwrap()
                    };
                    receipt.line("flushed_page", target.identity().page.page_no);
                    let name = if id == "MCA03-N02" {
                        "N02_ACTIVE_LEAF_HOME_SYNCED"
                    } else {
                        "N05_FIRST_FORWARD_HOME_SYNCED"
                    };
                    native::arm_row_cut(trace, name, &[], step("sync", HOME), RowCutMode::After);
                    receipt.line("armed", name);
                    let mut flushed = flush.try_flush(flusher, &target).unwrap();
                    for _ in 0..8 {
                        if matches!(
                            flushed,
                            RetentionFlushStep::Complete { .. } | RetentionFlushStep::NoDirty
                        ) {
                            break;
                        }
                        flushed = flush.continue_flush(flusher).unwrap();
                    }
                    panic!("{id}: the home sync cut was not reached ({flushed:?})");
                }
                "MCA03-N06" | "MCA03-N07" | "MCA03-N08" => {
                    match id {
                        "MCA03-N06" => {
                            native::arm_row_cut(
                                trace,
                                "N06_COMMIT_WRITE_ERROR",
                                &[],
                                step("write", WAL),
                                RowCutMode::WriteError,
                            );
                            receipt.line("armed", "N06_COMMIT_WRITE_ERROR");
                        }
                        "MCA03-N07" => {
                            native::arm_row_cut(
                                trace,
                                "N07_COMMIT_SYNC_ERROR",
                                &[step("write", WAL)],
                                step("sync", WAL),
                                RowCutMode::SyncError,
                            );
                            receipt.line("armed", "N07_COMMIT_SYNC_ERROR");
                        }
                        _ => {
                            native::note_arm(trace, "N08_COMMIT_BEFORE_PUBLISH");
                            receipt.line("armed", "N08_COMMIT_BEFORE_PUBLISH");
                            hook::arm_abort(core, Point::BeforePublish, 0);
                        }
                    }
                    let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                    match commit_live(&mut call, a2, private) {
                        Ok(committed) => panic!("{id}: Commit ACKed q {}", committed.q()),
                        Err(DriveError { failure, .. }) => {
                            // The caller's outcome is UNKNOWN; no ACK.
                            receipt.line("commit", format!("unknown {:?}", failure.stage));
                            kill(receipt, "COMMIT_UNKNOWN");
                        }
                    }
                }
                id if id.starts_with("MCA03-R") => {
                    // The R input: W's Commit and A2's Reserve/Forward durable.
                    kill(receipt, "R_INPUT");
                }
                _ => panic!("case {id}"),
            }
            let _ = (fence, core);
        }
    )
}
