//! S9 C+A native supply (boundaries `#row-mi03-s9`, consumers §8.2): the
//! actual native writer history of A (`produce`), C's recovery with A's
//! recovered CLR handler to READY and A's new row and Commit ACK, then the
//! re-entry, all without B. The READY read is C's page reader over the
//! READY files (`PageScan`). The S7 join (`row_join`, C+A+B) shares the
//! producer, the restart and the epoch oracle and reads with B's cursor
//! instead. Moved from `row_join` and `row_join::native` unchanged except
//! that the restart takes the READY read as a parameter.
use super::super::super::{Plan4RecoveryAcquireFailure, RowRecoveryPolicy};
use super::row_forward::VALUE;
use super::row_preflight::{create_table, system_init};
use super::row_recovery::leaf_rows;
use super::row_reopen::wal_witness;
use super::*;
use crate::storage::{
    disk::{
        IsolationLevel, RecoveryAction, RetentionCause, RetentionError, RowMutationObservation,
        TableStep,
    },
    retention_page::row::{self as job3, JobScope, JobState},
    row::insert::driver::{
        After, DriveError, DriverError, Failure, PendingRollback, Stepped,
        live::{
            LiveCall, commit_live, finish_rollback_live, finish_statement_live, insert_live,
            rollback_step_live, start_full_rollback_live, start_statement_rollback_live,
        },
        recovered::handle_recovered_clr,
    },
    row_record::ValueRef,
};
use std::{fs, path::Path};

/// C's error inside a failed A driver call.
pub(super) fn disk(failure: &Failure) -> &RetentionError {
    match &failure.error {
        DriverError::Disk(error) => error,
        DriverError::Insert(error) => panic!("A's {error:?}, not C's"),
    }
}

/// A failed driver call's pending rollback.
pub(super) fn rollback_required<H>(after: After<H>) -> PendingRollback<H> {
    match after {
        After::RollbackRequired(pending) => pending,
        _ => panic!("the call is not RollbackRequired"),
    }
}

/// One A driver statement inserting `values` (one row each): the rows'
/// observations. The driver finishes the statement.
macro_rules! a_insert {
    ($writer:expr, $owner:expr, $flush:ident, $flusher:ident, $view:expr, $table:expr, $private:expr, $values:expr) => {{
        let values: Vec<[ValueRef<'static>; 1]> = $values
            .iter()
            .map(|v: &i64| [ValueRef::Int64(*v)])
            .collect();
        let rows: Vec<&[ValueRef<'_>]> = values.iter().map(|v| &v[..]).collect();
        let mut slots: Vec<Option<RowMutationObservation>> = rows.iter().map(|_| None).collect();
        let mut call = LiveCall::new($writer, $owner, &mut $flush, $flusher);
        match insert_live(&mut call, $view, $table, $private, &rows, &mut slots) {
            Ok(n) => assert_eq!(n, rows.len()),
            Err(error) => panic!("A's statement refused {:?}", error.failure),
        }
        slots
            .into_iter()
            .map(|slot| slot.unwrap())
            .collect::<Vec<_>>()
    }};
}

/// A's Commit through the driver, which must succeed: its q.
macro_rules! a_commit {
    ($writer:expr, $owner:expr, $flush:ident, $flusher:ident, $tx:expr, $private:expr) => {{
        let mut call = LiveCall::new($writer, $owner, &mut $flush, $flusher);
        match commit_live(&mut call, $tx, $private) {
            Ok(committed) => committed.q(),
            Err(error) => panic!("A's Commit refused {:?}", error.failure),
        }
    }};
}

/// The Catalog epoch view of a session (`catalog_test_epochs`): active,
/// the namespace epoch and each table's (state, object, last change q).
pub(super) type Epochs = (bool, u64, Vec<(u8, u64, u64)>);

/// One restart's join result (`join_restart_with`).
pub(super) struct Joined {
    /// C's Progress actions to READY.
    pub(super) steps: usize,
    /// A's recovered CLRs to READY.
    pub(super) clrs: usize,
    /// The READY read's rows (RowId, value): B's cursor (`row_join`) or the
    /// page reader over the READY files (`PageScan`).
    pub(super) rows: Vec<(u64, i64)>,
    /// A's new row and its Commit q, when the restart ACKs one.
    pub(super) acked: Option<(u64, u64)>,
    /// The published Q and the epochs at the READY handoff.
    pub(super) ready: (u64, Epochs),
    /// The Row Job cells left (not Free) at the READY handoff: every Row
    /// root and terminal was cleaned before READY when this is 0.
    pub(super) row_cells: usize,
    /// The epochs after the read and A's Commit (if any).
    pub(super) after: Epochs,
}

/// The read of T after READY in `join_restart_with`.
pub(super) trait JoinRead {
    fn scan<'s, 'c, W: WriterNamespace>(
        &self,
        live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
        owner: &mut Owner<'c, 's>,
        flusher: &mut Owner<'c, 's>,
        reader_private: &mut [u8; PRIVATE_BYTES],
        ready_files: &Images,
    ) -> Vec<(u64, i64)>;
}

/// C's page reader over the READY files (`row_recovery::leaf_rows`), no B.
pub(super) struct PageScan;

impl JoinRead for PageScan {
    fn scan<'s, 'c, W: WriterNamespace>(
        &self,
        _: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
        _: &mut Owner<'c, 's>,
        _: &mut Owner<'c, 's>,
        _: &mut [u8; PRIVATE_BYTES],
        ready_files: &Images,
    ) -> Vec<(u64, i64)> {
        leaf_rows(ready_files)
    }
}

/// One recovery process over `directory`: C's recovery under `policy` to
/// READY (A's handler for each RowClr), `read`'s read of T, then, with
/// `ack`, A's new row of `ack` and its Commit. `files` takes the
/// directory's files (the READY files `PageScan` reads). The memory
/// MCI01-V11 case and the native MCI02 worker use B's cursor (`row_join`);
/// the C+A native worker (`row_native_a::native`) uses `PageScan`.
pub(super) fn join_restart_with<D: DirectoryPort, R: JoinRead>(
    directory: &D,
    policy: RowRecoveryPolicy,
    ack: Option<i64>,
    files: &dyn Fn() -> Images,
    read: &R,
) -> Joined {
    let mut maintenance = boxed::<12_582_912>();
    let mut dwb = boxed::<4_194_304>();
    let mut slot = boxed::<1_048_576>();
    let acquired = match Plan4Recovery::acquire(
        directory,
        Some(super::super::support::namespace()),
        &mut maintenance,
        &mut dwb,
        &mut slot,
    ) {
        Ok(owner) => owner,
        Err(Plan4RecoveryAcquireFailure::Acquired(_))
        | Err(Plan4RecoveryAcquireFailure::Before { .. }) => panic!("acquire"),
    };
    let runtime = acquired.recover_with_rows(policy).ok().unwrap();
    let core = PageConcurrency::new(runtime.session_fence()).unwrap();
    let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
    let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
    let mut backing = vec![[0xa7; PAGE_BYTES]; 16];
    let mut frames: Vec<_> = backing.iter_mut().map(DirtyFrame::new).collect();
    let mut operation = boxed::<114_688>();
    let mut closed = runtime
        .bind(&owner, &core, &mut frames, &mut operation)
        .unwrap();
    let (mut steps, mut clrs, mut ready) = (0, 0, false);
    for _ in 0..512 {
        match closed.next_action(&mut owner, &mut flusher).unwrap() {
            RecoveryAction::Ready => {
                ready = true;
                break;
            }
            RecoveryAction::Progress(progress) => {
                assert!(!progress.ready);
                steps += 1;
            }
            RecoveryAction::RowClr(task) => {
                let actor = task.actor();
                match handle_recovered_clr(task) {
                    Ok(progress) => assert!(progress.progressed && !progress.ready),
                    Err(_) => panic!("A's recovered CLR for {actor:?}"),
                }
                clrs += 1;
            }
        }
    }
    assert!(ready, "bounded Row recovery exhausted");
    let (mut live, private) = match closed.into_live_with_private() {
        Ok(value) => value,
        Err(_) => panic!("original READY handoff"),
    };
    let at_ready = (live.catalog_test_q(), live.catalog_test_epochs());
    let row_cells = live
        .catalog_test_jobs()
        .iter()
        .filter(|bytes| {
            let job = job3::decode_job(&bytes[..], super::super::support::namespace()).unwrap();
            job.scope() == Some(JobScope::Row) && job.state() != JobState::Free
        })
        .count();
    let ready_files = files();
    let row_status = live.recovery_test_expected_target(11).unwrap();
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
    prepare_catalog(&mut live, &owner, &mut flusher);
    if ack.is_some() {
        system_init(&mut live, &mut owner, &mut flusher, true);
    }
    let mut reader_private = boxed::<PRIVATE_BYTES>();
    let reader_private = &mut *reader_private;
    let owner = &mut owner;
    let flusher = &mut flusher;
    let rows = read.scan(&mut live, owner, flusher, reader_private, &ready_files);
    let mut acked = None;
    {
        let (mut writer, reader, mut flush) = live.split();
        if let Some(value) = ack {
            let a = writer
                .begin_row_transaction(
                    owner,
                    IsolationLevel::Serializable,
                    &row_status,
                    private,
                    &mut flush,
                )
                .unwrap();
            let s = writer.begin_statement(owner, &a).unwrap();
            let table = issue_table!(reader, owner, s, reader_private, flush, flusher);
            let new = a_insert!(
                &mut writer,
                owner,
                flush,
                flusher,
                s,
                &table,
                private,
                [value]
            );
            let q = a_commit!(&mut writer, owner, flush, flusher, a, private);
            acked = Some((new[0].row().get(), q));
        }
        assert_eq!(reader.catalog_test_source_used(), 0);
        owner.check_idle().unwrap();
    }
    let after = live.catalog_test_epochs();
    assert!(!runtime.session_fence().is_fenced());
    drop(live);
    Joined {
        steps,
        clrs,
        rows,
        acked,
        ready: at_ready,
        row_cells,
        after,
    }
}

/// The logical-epoch oracle of a restart whose history's last ACKed Commit
/// is `q` (MCI01-V11): READY's exclusive activation seeds the namespace
/// epoch and T's epoch with the published Q (task-10
/// `#module-row-isolation`), and that Q is still `q` after every physical
/// cleanup the recovery ran (Prune, ReleaseRole, RootFree, TerminalFinish,
/// and a loser's CLRs and AbortEnd), all of which completed before READY
/// (no Row Job cell is left). The epoch ledger is inactive during recovery,
/// so V11 holds there by construction; what this checks is that no cleanup
/// step published a q. A later Row Commit at `q'` moves only T's epoch
/// (V10).
pub(super) fn check_epochs(joined: &Joined, q: u64) {
    assert_eq!(joined.row_cells, 0, "Row cleanup finished before READY");
    assert_eq!(joined.ready, (q, (true, q, vec![(2, 6, q)])));
    let t = joined.acked.map_or(q, |(_, acked)| acked);
    assert_eq!(joined.after, (true, q, vec![(2, 6, t)]));
}

/// The value of the row each recovery process commits after READY.
pub(super) const NEW: i64 = VALUE + 10;

/// PID1: the actual native writer history of `id`, stopped at its cut. The
/// receipt holds the ACKed q and the WAL witness at the cut; the process
/// then exits with no flush or further write (Drop releases the lease and
/// closes the handles only, which the controller checks).
pub(super) fn produce<D: DirectoryPort>(id: &str, directory: &D, db: &Path) -> String {
    with_directory!(directory, |live,
                                owner,
                                flusher,
                                status,
                                undo,
                                private,
                                reader_private,
                                core,
                                fence| {
        let (row_status, _) = create_table(live, owner, flusher, status, undo, private, true);
        let other_status = (id == "MCI02-N03").then(|| system_init(live, owner, flusher, false));
        for _ in 0..match id {
            "MCI02-N01" => 0,
            "MCI02-N02" => 2,
            _ => 3,
        } {
            system_init(live, owner, flusher, true);
        }
        let (mut writer, reader, mut flush) = live.split();
        let a = writer
            .begin_row_transaction(
                owner,
                IsolationLevel::Serializable,
                &row_status,
                private,
                &mut flush,
            )
            .unwrap();
        let s1 = writer.begin_statement(owner, &a).unwrap();
        let table = issue_table!(reader, owner, s1, reader_private, flush, flusher);
        let summary = match id {
            "MCI02-N01" => {
                let rows = a_insert!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    s1,
                    &table,
                    private,
                    [VALUE]
                );
                let q = a_commit!(&mut writer, owner, flush, flusher, a, private);
                let cut = wal_witness(db);
                format!(
                    "result\tacked\nq\t{q}\nrows\t{}\nwal_cut\t{}\nwal_cut_bytes\t{}\n",
                    rows[0].row().get(),
                    cut.0,
                    cut.1
                )
            }
            "MCI02-N02" => {
                let kept = a_insert!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    s1,
                    &table,
                    private,
                    [VALUE]
                );
                let s2 = writer.begin_statement(owner, &a).unwrap();
                let good = [ValueRef::Int64(VALUE + 1)];
                let bad = [ValueRef::Null];
                let input: [&[ValueRef<'_>]; 2] = [&good, &bad];
                let mut slots: [Option<RowMutationObservation>; 2] = [None, None];
                {
                    let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                    let mut pending =
                        match insert_live(&mut call, s2, &table, private, &input, &mut slots) {
                            Err(DriveError { failure, after }) => {
                                assert!(
                                    matches!(failure.error, DriverError::Insert(_)),
                                    "{failure:?}"
                                );
                                rollback_required(after)
                            }
                            Ok(n) => panic!("{n} rows inserted"),
                        };
                    {
                        let mut rollback = match start_statement_rollback_live(
                            &mut call,
                            &a,
                            &mut pending,
                            private,
                        ) {
                            Ok(rollback) => rollback,
                            Err(failure) => panic!("statement RollbackStart {failure:?}"),
                        };
                        assert!(matches!(
                            rollback_step_live(&mut rollback),
                            Ok(Stepped::Compensated)
                        ));
                        assert!(matches!(
                            rollback_step_live(&mut rollback),
                            Ok(Stepped::Complete)
                        ));
                        assert!(finish_rollback_live(rollback).is_ok());
                    }
                    let s2 = match pending.rolled_back() {
                        Ok(view) => view,
                        Err(_) => panic!("statement rollback incomplete"),
                    };
                    assert!(finish_statement_live(&mut call, s2).is_ok());
                }
                let q = a_commit!(&mut writer, owner, flush, flusher, a, private);
                let cut = wal_witness(db);
                format!(
                    "result\tacked\nq\t{q}\nrows\t{},{}\nwal_cut\t{}\nwal_cut_bytes\t{}\n",
                    kept[0].row().get(),
                    slots[0].as_ref().unwrap().row().get(),
                    cut.0,
                    cut.1
                )
            }
            "MCI02-N03" => {
                let rows = a_insert!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    s1,
                    &table,
                    private,
                    [VALUE, VALUE + 1, VALUE + 2]
                );
                let y = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::ReadCommitted,
                        other_status.as_ref().unwrap(),
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
                    [VALUE + 3]
                );
                let y_q = a_commit!(&mut writer, owner, flush, flusher, y, private);
                let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                let mut pending = match commit_live(&mut call, a, private) {
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
                let before = wal_witness(db);
                assert!(matches!(
                    rollback_step_live(&mut rollback),
                    Ok(Stepped::Compensated)
                ));
                // The CLR's apply returned after its frame was durable: the
                // cut is D (RollbackStart) + F (CLR1); the process stops here.
                let after = wal_witness(db);
                assert_ne!(before, after, "CLR1 reached the WAL file");
                let summary = format!(
                    "result\tcrashed\nq\t{y_q}\nrows\t{},{},{},{}\nwal_before_clr\t{}\n\
                     wal_cut\t{}\nwal_cut_bytes\t{}\n",
                    rows[0].row().get(),
                    rows[1].row().get(),
                    rows[2].row().get(),
                    y_rows[0].row().get(),
                    before.0,
                    after.0,
                    after.1
                );
                let _ = (fence, core);
                return summary;
            }
            _ => panic!("case {id}"),
        };
        assert!(!fence.is_fenced());
        let _ = core;
        summary
    })
}

/// The rows a READY read must find after each phase, fixed by the case's history:
/// (value) in RowId order; the new row's value is NEW.
pub(super) fn expected(id: &str, phase: &str) -> Vec<i64> {
    let committed = match id {
        "MCI02-N01" => vec![VALUE],
        "MCI02-N02" => vec![VALUE],
        "MCI02-N03" => vec![VALUE + 3],
        _ => panic!("case {id}"),
    };
    match phase {
        "recover" => committed,
        "reenter" => committed.into_iter().chain([NEW]).collect(),
        _ => panic!("phase {phase}"),
    }
}

#[cfg(target_os = "linux")]
#[path = "row_native_a/native.rs"]
mod native;
